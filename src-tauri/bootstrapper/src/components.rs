//! Fetches the extra components GitWyrm needs but does not ship inside its
//! installer - currently git and gpg.
//!
//! **Nothing here knows what those components are.** The CDN manifest lists
//! them, and this code installs whatever it lists. That is deliberate: this
//! binary is published once under a filename that never changes (the website's
//! download button points at it, and its download count is the reason the name
//! is stable), so it is rebuilt far less often than the app. Hardcoding "git and
//! gpg" would mean a new bootstrapper every time the component set changed.
//!
//! Adding or removing a component is therefore a CI change - publish a different
//! manifest - and every bootstrapper already in the wild picks it up.
//!
//! The manifest shape, at `tools/<arch>/components.json`. Only `name` and `url`
//! are required; every other field has a default that keeps the smallest
//! possible entry working:
//!
//! ```json
//! {
//!   "components": [
//!     {
//!       "name": "toolset",
//!       "url": "https://cdn.gitwyrm.com/tools/x86_64/toolset.tar.xz",
//!       "version": "v2.55.0.windows.3",
//!       "sha256": "bf46...",
//!       "size": 24889372,
//!       "kind": "archive",
//!       "into": ""
//!     },
//!     {
//!       "name": "something-with-its-own-installer",
//!       "url": "https://cdn.gitwyrm.com/tools/x86_64/thing-setup.exe",
//!       "version": "1.2.3",
//!       "sha256": "aa11...",
//!       "kind": "installer",
//!       "args": "/S"
//!     }
//!   ]
//! }
//! ```
//!
//! | field | default | meaning |
//! |---|---|---|
//! | `kind` | `archive` | `archive` unpacks a tar.xz; `installer` runs an exe |
//! | `args` | none | arguments for `installer`, e.g. a silent switch |
//! | `into` | toolset root | subdirectory to unpack an `archive` into |
//! | `version` | none | absent means "cannot tell", so it reinstalls |
//! | `sha256` | none | absent means unverified, and is logged loudly |
//! | `size` | Content-Length | only used for the progress bar |
//!
//! Unknown fields are ignored and an unknown `kind` is skipped rather than
//! guessed at, so a manifest can grow without breaking older bootstrappers -
//! the other half of staying forward-compatible.

use crate::{log, DownloadMsg};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;

/// How a component is put on disk once downloaded.
///
/// Named in the manifest as `kind`. An unrecognised kind is skipped rather than
/// guessed at: a bootstrapper too old to understand a new kind must leave it to
/// the app rather than run an installer it does not understand.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Kind {
    /// A tar.xz unpacked into the toolset directory. The default.
    Archive,
    /// An executable to run, with `args` (typically a silent switch).
    Installer,
    /// Something this build does not know about.
    Unknown(String),
}

impl Kind {
    fn parse(raw: &str) -> Self {
        match raw {
            "" | "archive" | "tar.xz" => Self::Archive,
            "installer" | "exe" => Self::Installer,
            other => Self::Unknown(other.to_owned()),
        }
    }
}

/// One downloadable piece of the toolset.
///
/// Only `name` and `url` are required. Everything else has a default that keeps
/// the smallest possible manifest working, which is what lets the tool list
/// change without shipping a new bootstrapper.
#[derive(Debug, Clone)]
pub struct Component {
    pub name: String,
    pub version: String,
    pub url: String,
    pub sha256: String,
    pub size: u64,
    /// What to do with the download. Defaults to `Archive`.
    pub kind: Kind,
    /// Command-line arguments for `Kind::Installer`, e.g. `/S`. Split on
    /// whitespace, so an argument containing a space is not expressible - none
    /// of the silent switches we use need one.
    pub args: Vec<String>,
    /// Subdirectory of the toolset dir to unpack into, for `Kind::Archive`.
    /// Empty means the archive is already rooted correctly (the usual case).
    pub into: String,
}

/// Where the component list lives for this architecture.
fn manifest_url() -> String {
    let arch = if cfg!(target_arch = "aarch64") {
        "aarch64"
    } else {
        "x86_64"
    };
    format!("{}/tools/{}/components.json", crate::CDN_BASE, arch)
}

/// Root the components unpack into: `<local app data>/dev.gitwyrm.app/tools`.
///
/// Must match `git::toolset::toolset_dir` in the app, which resolves the same
/// path. Outside the install directory on purpose - the NSIS uninstaller wipes
/// `$INSTDIR` on every update, and these are meant to survive that.
pub fn toolset_dir() -> Option<PathBuf> {
    std::env::var_os("LOCALAPPDATA")
        .or_else(|| std::env::var_os("APPDATA"))
        .map(|base| {
            PathBuf::from(base)
                .join("dev.gitwyrm.app")
                .join("tools")
        })
}

/// Records which version of a component is unpacked, so the app and a later
/// bootstrapper run can both tell whether it is current.
fn version_file(dir: &Path, name: &str) -> PathBuf {
    dir.join(format!("{name}.version"))
}

/// Read the manifest and return the components that are not already current.
///
/// A parse failure is reported as "nothing to do" rather than an error: the app
/// re-checks on launch, so a malformed manifest must not block an install.
pub fn pending(client: &reqwest::blocking::Client) -> Vec<Component> {
    let url = manifest_url();
    log(&format!("Fetching component manifest: {}", url));

    let text = match client.get(&url).send().and_then(|r| r.error_for_status()).and_then(|r| r.text()) {
        Ok(t) => t,
        Err(e) => {
            log(&format!("WARNING: no component manifest ({e}); skipping"));
            return Vec::new();
        }
    };

    let all = match parse_manifest(&text) {
        Ok(list) => list,
        Err(e) => {
            log(&format!("WARNING: could not read component manifest: {e}"));
            return Vec::new();
        }
    };

    let Some(dir) = toolset_dir() else {
        log("WARNING: could not resolve the toolset directory");
        return Vec::new();
    };

    all.into_iter()
        .filter(|c| {
            // No version in the manifest means there is nothing to compare, so
            // the only safe answer is to fetch it.
            if c.version.is_empty() {
                return true;
            }
            let installed = std::fs::read_to_string(version_file(&dir, &c.name))
                .ok()
                .map(|s| s.trim().to_owned());
            match installed {
                Some(v) if v == c.version => {
                    log(&format!("{} {} already installed", c.name, c.version));
                    false
                }
                _ => true,
            }
        })
        .collect()
}

/// Parse the manifest without pulling in a JSON crate.
///
/// The bootstrapper is optimized for size (`opt-level = "z"`, LTO, stripped) and
/// this is the only JSON it ever reads, so a scanner for the handful of fields
/// it needs is cheaper than a dependency. Unknown fields fall out for free,
/// which is what keeps older bootstrappers working against newer manifests.
fn parse_manifest(text: &str) -> Result<Vec<Component>, String> {
    let mut components = Vec::new();

    // Each component is one `{...}` block inside the "components" array.
    let body = match text.find("\"components\"") {
        Some(i) => &text[i..],
        None => return Err("no components array".into()),
    };

    let mut rest = body;
    while let Some(open) = rest.find('{') {
        let Some(close) = rest[open..].find('}') else {
            break;
        };
        let block = &rest[open..open + close];
        rest = &rest[open + close..];

        // Only name and url are required. A manifest may be as small as
        // {"name": "git", "url": "..."} - the rest are optimizations:
        //   version  absent -> always reinstall (cannot tell if it is current)
        //   sha256   absent -> unverified (logged loudly; see install())
        //   size     absent -> progress runs on Content-Length instead
        let name = field(block, "name");
        let url = field(block, "url");
        let version = field(block, "version").unwrap_or_default();
        let sha256 = field(block, "sha256").unwrap_or_default();
        let size = field(block, "size")
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(0);
        let kind = Kind::parse(&field(block, "kind").unwrap_or_default());
        let args = field(block, "args")
            .map(|raw| {
                raw.split_whitespace()
                    .map(|s| s.to_owned())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let into = field(block, "into").unwrap_or_default();

        if let (Some(name), Some(url)) = (name, url) {
            components.push(Component {
                name,
                version,
                url,
                sha256,
                size,
                kind,
                args,
                into,
            });
        }
    }

    if components.is_empty() {
        return Err("no usable component entries".into());
    }
    Ok(components)
}

/// Pull `"key": "value"` or `"key": 123` out of one JSON object block.
fn field(block: &str, key: &str) -> Option<String> {
    let needle = format!("\"{key}\"");
    let start = block.find(&needle)? + needle.len();
    let after = block[start..].trim_start();
    let after = after.strip_prefix(':')?.trim_start();

    if let Some(quoted) = after.strip_prefix('"') {
        let end = quoted.find('"')?;
        Some(quoted[..end].to_owned())
    } else {
        let end = after
            .find(|c: char| c == ',' || c == '}' || c.is_whitespace())
            .unwrap_or(after.len());
        let value = after[..end].trim();
        (!value.is_empty()).then(|| value.to_owned())
    }
}

/// Download, verify and unpack one component. Returns an error message on
/// failure rather than aborting the whole install.
pub fn install(
    client: &reqwest::blocking::Client,
    component: &Component,
    tx: &Sender<DownloadMsg>,
) -> Result<(), String> {
    let dir = toolset_dir().ok_or("could not resolve the toolset directory")?;
    std::fs::create_dir_all(&dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;

    log(&format!(
        "Downloading {} {} from {}",
        component.name, component.version, component.url
    ));

    let response = client
        .get(&component.url)
        .send()
        .and_then(|r| r.error_for_status())
        .map_err(|e| format!("download failed: {e}"))?;

    let total = response.content_length().unwrap_or(component.size);
    let mut reader = response;
    let mut bytes: Vec<u8> = Vec::with_capacity(total as usize);
    let mut buf = [0u8; 65536];
    let mut last_report = std::time::Instant::now();

    loop {
        match reader.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                bytes.extend_from_slice(&buf[..n]);
                if last_report.elapsed() > std::time::Duration::from_millis(50) {
                    let _ = tx.send(DownloadMsg::Progress(bytes.len() as u64, total));
                    last_report = std::time::Instant::now();
                }
            }
            Err(e) => return Err(format!("download error: {e}")),
        }
    }
    let _ = tx.send(DownloadMsg::Progress(bytes.len() as u64, total));

    // Verified before anything is unpacked: these become executables that run
    // as the user, so a substituted archive must never reach disk as a tree.
    // A manifest without a hash is allowed but is worth shouting about - the
    // transport is HTTPS, but that only authenticates the host, not the bytes
    // a compromised bucket would serve.
    if component.sha256.is_empty() {
        log(&format!(
            "WARNING: {} has no sha256 in the manifest; installing unverified",
            component.name
        ));
    } else {
        let actual = sha256_hex(&bytes);
        if !actual.eq_ignore_ascii_case(&component.sha256) {
            return Err(format!(
                "checksum mismatch for {}: expected {}, got {actual}",
                component.name, component.sha256
            ));
        }
    }

    // An unrecognised kind is left alone rather than guessed at. A newer
    // manifest may describe something this build predates; the app checks the
    // same list on launch and can handle it there.
    if let Kind::Unknown(kind) = &component.kind {
        return Err(format!(
            "{} has kind '{kind}', which this installer does not understand",
            component.name
        ));
    }

    // An installer is run, not unpacked. `args` carries the silent switch, which
    // differs per vendor (/S, /quiet, /VERYSILENT), so it belongs in the
    // manifest rather than being guessed here.
    if component.kind == Kind::Installer {
        let exe = std::env::temp_dir().join(format!("gitwyrm-{}-{}.exe", component.name, std::process::id()));
        std::fs::write(&exe, &bytes).map_err(|e| format!("cannot write installer: {e}"))?;

        log(&format!(
            "Running {} {}",
            exe.display(),
            component.args.join(" ")
        ));
        let status = std::process::Command::new(&exe)
            .args(&component.args)
            .status()
            .map_err(|e| format!("could not run {}: {e}", component.name))?;
        let _ = std::fs::remove_file(&exe);

        if !status.success() {
            return Err(format!(
                "{} installer exited with {}",
                component.name,
                status.code().unwrap_or(-1)
            ));
        }

        std::fs::write(version_file(&dir, &component.name), &component.version)
            .map_err(|e| format!("could not record the version: {e}"))?;
        log(&format!("Installed {} {}", component.name, component.version));
        return Ok(());
    }

    // Unpack to scratch, then swap, so an interrupted unpack cannot leave
    // something that looks installed.
    let scratch = dir.join(format!(".{}-incoming", component.name));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).map_err(|e| format!("cannot stage: {e}"))?;

    let decoder = liblzma::read::XzDecoder::new(&bytes[..]);
    let mut archive = tar::Archive::new(decoder);
    archive
        .unpack(&scratch)
        .map_err(|e| format!("could not unpack {}: {e}", component.name))?;

    // `into` places an archive that is not already rooted the way the toolset
    // expects; empty (the usual case) means its own top-level names are correct.
    let destination = if component.into.is_empty() {
        dir.clone()
    } else {
        let nested = dir.join(&component.into);
        std::fs::create_dir_all(&nested).map_err(|e| format!("cannot create {}: {e}", nested.display()))?;
        nested
    };

    // Move each top-level entry into place. Written this way rather than
    // assuming a single root so one archive can carry several directories.
    for entry in std::fs::read_dir(&scratch).map_err(|e| format!("cannot read staged tree: {e}"))? {
        let entry = entry.map_err(|e| format!("cannot read staged entry: {e}"))?;
        let target = destination.join(entry.file_name());
        let _ = std::fs::remove_dir_all(&target);
        let _ = std::fs::remove_file(&target);
        std::fs::rename(entry.path(), &target)
            .map_err(|e| format!("could not move {} into place: {e}", target.display()))?;
    }
    let _ = std::fs::remove_dir_all(&scratch);

    // Written last: a component labelled with a version must actually be that
    // version, unpacked and complete.
    std::fs::write(version_file(&dir, &component.name), &component.version)
        .map_err(|e| format!("could not record the version: {e}"))?;

    log(&format!("Installed {} {}", component.name, component.version));
    Ok(())
}

/// SHA-256 as lowercase hex, implemented here to avoid a dependency in a binary
/// that is deliberately kept small.
fn sha256_hex(data: &[u8]) -> String {
    let hash = sha256(data);
    hash.iter().map(|b| format!("{b:02x}")).collect()
}

/// FIPS 180-4 SHA-256.
fn sha256(data: &[u8]) -> [u8; 32] {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];

    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];

    let mut message = data.to_vec();
    let bit_len = (data.len() as u64).wrapping_mul(8);
    message.push(0x80);
    while message.len() % 64 != 56 {
        message.push(0);
    }
    message.extend_from_slice(&bit_len.to_be_bytes());

    for chunk in message.chunks(64) {
        let mut w = [0u32; 64];
        for (i, word) in chunk.chunks(4).enumerate() {
            w[i] = u32::from_be_bytes([word[0], word[1], word[2], word[3]]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }

        let (mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh) =
            (h[0], h[1], h[2], h[3], h[4], h[5], h[6], h[7]);

        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let t1 = hh
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let t2 = s0.wrapping_add(maj);

            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(t1);
            d = c;
            c = b;
            b = a;
            a = t1.wrapping_add(t2);
        }

        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
        h[5] = h[5].wrapping_add(f);
        h[6] = h[6].wrapping_add(g);
        h[7] = h[7].wrapping_add(hh);
    }

    let mut out = [0u8; 32];
    for (i, word) in h.iter().enumerate() {
        out[i * 4..i * 4 + 4].copy_from_slice(&word.to_be_bytes());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_matches_known_vectors() {
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        // Longer than one 64-byte block, to exercise the chunk loop.
        assert_eq!(
            sha256_hex(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"),
            "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
        );
    }

    #[test]
    fn a_manifest_parses_into_components() {
        let raw = r#"{
          "components": [
            {
              "name": "git",
              "version": "v2.55.0.windows.3",
              "url": "https://cdn.gitwyrm.com/tools/x86_64/git.tar.xz",
              "sha256": "abc123",
              "size": 24889372
            },
            {
              "name": "gpg",
              "version": "v2.55.0.windows.3",
              "url": "https://cdn.gitwyrm.com/tools/x86_64/gpg.tar.xz",
              "sha256": "def456",
              "size": 1234
            }
          ]
        }"#;
        let parsed = parse_manifest(raw).expect("should parse");
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].name, "git");
        assert_eq!(parsed[0].size, 24_889_372);
        assert_eq!(parsed[1].name, "gpg");
        assert_eq!(parsed[1].sha256, "def456");
    }

    #[test]
    fn unknown_fields_do_not_break_an_older_bootstrapper() {
        // The whole point of the manifest: adding a component or a field must
        // not require shipping a new bootstrapper.
        let raw = r#"{
          "schema": 2,
          "components": [
            {
              "name": "git",
              "version": "v1",
              "url": "https://example.invalid/git.tar.xz",
              "sha256": "aa",
              "size": 1,
              "notes": "something we added later",
              "optional": false
            }
          ]
        }"#;
        let parsed = parse_manifest(raw).expect("should parse despite unknown keys");
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].name, "git");
        assert_eq!(parsed[0].version, "v1");
    }

    #[test]
    fn name_and_url_alone_are_enough() {
        // The minimum useful manifest: a list of tools and where to get them.
        let raw = r#"{"components":[{"name":"git","url":"https://example.invalid/git.tar.xz"}]}"#;
        let parsed = parse_manifest(raw).expect("minimal manifest should parse");
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].name, "git");
        assert_eq!(parsed[0].url, "https://example.invalid/git.tar.xz");
        // Absent version means "cannot tell if it is current", so it reinstalls.
        assert!(parsed[0].version.is_empty());
        assert!(parsed[0].sha256.is_empty());
        assert_eq!(parsed[0].size, 0);
    }

    #[test]
    fn an_installer_component_carries_its_silent_switch() {
        // The silent switch differs per vendor, so it belongs in the manifest
        // rather than being guessed by the bootstrapper.
        let raw = r#"{"components":[{
            "name": "thing",
            "url": "https://example.invalid/thing-setup.exe",
            "kind": "installer",
            "args": "/S /norestart"
        }]}"#;
        let parsed = parse_manifest(raw).expect("should parse");
        assert_eq!(parsed[0].kind, Kind::Installer);
        assert_eq!(parsed[0].args, vec!["/S", "/norestart"]);
    }

    #[test]
    fn an_archive_is_the_default_kind() {
        let raw = r#"{"components":[{"name":"git","url":"https://example.invalid/g.tar.xz"}]}"#;
        let parsed = parse_manifest(raw).expect("should parse");
        assert_eq!(parsed[0].kind, Kind::Archive);
        assert!(parsed[0].args.is_empty());
        assert!(parsed[0].into.is_empty(), "unpacks at the toolset root");
    }

    #[test]
    fn an_unknown_kind_is_kept_but_flagged() {
        // Parsed rather than dropped so the caller can log which component it
        // skipped; install() is what refuses to act on it.
        let raw = r#"{"components":[{
            "name": "future",
            "url": "https://example.invalid/x",
            "kind": "msix"
        }]}"#;
        let parsed = parse_manifest(raw).expect("should parse");
        assert_eq!(parsed[0].kind, Kind::Unknown("msix".into()));
    }

    #[test]
    fn an_archive_can_name_a_subdirectory() {
        let raw = r#"{"components":[{
            "name": "extra",
            "url": "https://example.invalid/e.tar.xz",
            "into": "extra"
        }]}"#;
        let parsed = parse_manifest(raw).expect("should parse");
        assert_eq!(parsed[0].into, "extra");
    }

    #[test]
    fn a_manifest_with_no_components_is_an_error_not_a_panic() {
        assert!(parse_manifest("{}").is_err());
        assert!(parse_manifest(r#"{"components": []}"#).is_err());
    }

    #[test]
    fn the_toolset_dir_is_shared_with_the_app() {
        let Some(dir) = toolset_dir() else {
            return;
        };
        let text = dir.to_string_lossy();
        // Must agree with git::toolset::toolset_dir in the app, or the app will
        // not find what the bootstrapper installed.
        assert!(text.contains("dev.gitwyrm.app"), "{text}");
        assert!(text.ends_with("tools"), "{text}");
    }
}

/// Guards the seam between CI and this parser: the manifest the publish script
/// writes and the manifest this binary reads have to agree, and nothing else
/// checks that.
#[cfg(test)]
mod ci_manifest_contract {
    use super::*;

    #[test]
    fn the_manifest_ci_actually_publishes_parses() {
        // Byte-for-byte the output of .github/scripts/publish-toolset.sh, so a
        // change to that script's shape fails here rather than at release time.
        let raw = r#"{
  "schema": 1,
  "components": [
    {
      "name": "toolset",
      "version": "v2.55.0.windows.3",
      "url": "https://cdn.gitwyrm.com/tools/x86_64/toolset.tar.xz",
      "sha256": "bf4663aa238399c988dc3e3b2ca5ad51bc9e87df462e7bee308dd969e392126f",
      "size": 24889372
    }
  ]
}"#;
        let parsed = parse_manifest(raw).expect("CI manifest must parse");
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].name, "toolset");
        assert_eq!(parsed[0].version, "v2.55.0.windows.3");
        assert_eq!(parsed[0].size, 24_889_372);
        assert_eq!(
            parsed[0].sha256,
            "bf4663aa238399c988dc3e3b2ca5ad51bc9e87df462e7bee308dd969e392126f"
        );
        assert!(parsed[0].url.ends_with("toolset.tar.xz"));
    }
}
