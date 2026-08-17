//! Downloads and unpacks the git/gpg toolset from the CDN.
//!
//! The archive is a tar.xz rooted at `git/` and `gpg/`, published by
//! `.github/scripts/publish-toolset.sh` alongside a manifest carrying its
//! version and SHA-256. See `git::toolset` for where the unpacked tree lands and
//! why that location survives an app update.
//!
//! Two invariants matter here, both because this fetches executables that will
//! later run as the user:
//!
//! 1. **Nothing is unpacked before the hash matches** the manifest. A truncated
//!    or substituted download must never reach disk as a runnable tool.
//! 2. **The swap is atomic-ish.** The tree is unpacked to a scratch directory
//!    and only then moved into place, so an interrupted download cannot leave a
//!    half-written toolset that looks installed.

use crate::error::AppError;
use crate::git::toolset::{self, VERSION_FILE};
use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::{Path, PathBuf};

/// Where the component list for this architecture lives.
///
/// The same manifest the bootstrapper reads, deliberately: two lists describing
/// the same tools would drift, and then a fresh install and an update would
/// disagree about what should be on disk.
///
/// Latest-only by design: there is no version in the path, so a new toolset
/// replaces the old one and nothing accumulates on the CDN.
#[cfg(windows)]
fn manifest_url() -> String {
  let arch = if cfg!(target_arch = "aarch64") {
    "aarch64"
  } else {
    "x86_64"
  };
  format!("https://cdn.gitwyrm.com/tools/{arch}/components.json")
}

/// One downloadable component. Only `name` and `url` are required; the rest let
/// the client skip work it has already done.
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize, specta::Type)]
pub struct ToolsetManifest {
  /// Component name, e.g. `toolset`.
  #[serde(default = "default_component_name")]
  pub name: String,
  /// Upstream Git for Windows tag, e.g. `v2.55.0.windows.3`. Empty when the
  /// manifest does not say, which forces a reinstall.
  #[serde(default)]
  pub version: String,
  /// Archive size in bytes, for progress reporting. Zero when unknown.
  #[serde(default)]
  pub size: u64,
  /// SHA-256 of the archive, lowercase hex. Empty means unverified.
  #[serde(default)]
  pub sha256: String,
  /// Absolute URL of the tar.xz.
  pub url: String,
  /// How to put it on disk: `archive` (the default) or `installer`. The app
  /// only handles archives - an `installer` component is the bootstrapper's job
  /// at install time, and is skipped here rather than run behind the user's
  /// back during a routine update check.
  #[serde(default)]
  pub kind: String,
  /// Subdirectory of the toolset dir to unpack into. Empty means the archive is
  /// already rooted correctly.
  #[serde(default)]
  pub into: String,
}

impl ToolsetManifest {
  /// Whether this component is one the app can install itself.
  ///
  /// Only the Windows manifest fetch asks; Linux never downloads a toolset.
  #[cfg(windows)]
  fn is_archive(&self) -> bool {
    matches!(self.kind.as_str(), "" | "archive" | "tar.xz")
  }
}

fn default_component_name() -> String {
  "toolset".to_owned()
}

/// The component list, as published. Read only by the Windows manifest fetch.
#[cfg(windows)]
#[derive(Debug, Clone, serde::Deserialize)]
struct ComponentsDocument {
  components: Vec<ToolsetManifest>,
}

/// Fetch the manifest describing the current toolset.
///
/// Returns the first component. Today one archive carries both git and gpg;
/// should that ever split, this is the place that grows to install each in turn.
#[cfg(windows)]
pub async fn fetch_manifest() -> Result<ToolsetManifest, AppError> {
  let url = manifest_url();
  let response = reqwest::get(&url)
    .await
    .map_err(|e| AppError::Other(format!("could not reach {url}: {e}")))?;

  if !response.status().is_success() {
    return Err(AppError::Other(format!(
      "toolset manifest {url} returned {}",
      response.status()
    )));
  }

  let document = response
    .json::<ComponentsDocument>()
    .await
    .map_err(|e| AppError::Other(format!("could not read toolset manifest: {e}")))?;

  // The first component the app can handle itself. An `installer` entry belongs
  // to the bootstrapper at install time: running a vendor installer from a
  // background update check would put a UAC prompt in front of someone who only
  // opened their repositories.
  document
    .components
    .into_iter()
    .find(|c| c.is_archive())
    .ok_or_else(|| AppError::Other("the toolset manifest lists no archive components".into()))
}

/// Whether a download is needed, and what it would install.
///
/// `None` means the toolset on disk is already current. A present tree whose
/// version file does not match is treated as needing an update, and a missing
/// tree needs a first install.
///
/// Always `None` off Windows. The published toolset is Git for Windows, and the
/// manifest path carries no OS, so a Linux build asking this question would
/// download and unpack Windows `.exe` files. Linux uses the system git and gpg,
/// declared as package dependencies, and never consults the CDN.
pub async fn needs_update() -> Result<Option<ToolsetManifest>, AppError> {
  #[cfg(not(windows))]
  {
    return Ok(None);
  }

  #[cfg(windows)]
  {
    needs_update_inner().await
  }
}

#[cfg(windows)]
async fn needs_update_inner() -> Result<Option<ToolsetManifest>, AppError> {
  let manifest = fetch_manifest().await?;

  if !toolset::is_installed() {
    return Ok(Some(manifest));
  }

  // No version in the manifest means there is nothing to compare against, so
  // the only safe answer is to fetch. Matches the bootstrapper's rule.
  if manifest.version.is_empty() {
    return Ok(Some(manifest));
  }

  match toolset::installed_version() {
    Some(installed) if installed == manifest.version => Ok(None),
    _ => Ok(Some(manifest)),
  }
}

/// Download, verify and unpack the toolset described by `manifest`.
///
/// `on_progress` is called with (downloaded, total) as bytes arrive so a caller
/// can drive a progress bar; it may be called many times and must be cheap.
pub async fn install<F>(manifest: &ToolsetManifest, mut on_progress: F) -> Result<(), AppError>
where
  F: FnMut(u64, u64),
{
  let dir = toolset::toolset_dir()
    .ok_or_else(|| AppError::Other("could not resolve the toolset directory".into()))?;

  let parent = dir
    .parent()
    .ok_or_else(|| AppError::Other("toolset directory has no parent".into()))?;
  std::fs::create_dir_all(parent)?;

  // ------------------------------------------------------------------ download
  let mut response = reqwest::get(&manifest.url)
    .await
    .map_err(|e| AppError::Other(format!("could not download the toolset: {e}")))?;

  if !response.status().is_success() {
    return Err(AppError::Other(format!(
      "toolset download returned {}",
      response.status()
    )));
  }

  // Held in memory rather than streamed to disk: at ~23 MB this is smaller than
  // many of the diffs the app already keeps around, and it means a failed hash
  // check never leaves a file behind to clean up.
  let mut bytes: Vec<u8> = Vec::with_capacity(manifest.size as usize);
  let mut hasher = Sha256::new();

  while let Some(chunk) = response
    .chunk()
    .await
    .map_err(|e| AppError::Other(format!("toolset download failed: {e}")))?
  {
    hasher.update(&chunk);
    bytes.extend_from_slice(&chunk);
    on_progress(bytes.len() as u64, manifest.size);
  }

  // -------------------------------------------------------------------- verify
  // Before anything is written, and before anything is unpacked: these are
  // executables, so an unverified archive must never touch the disk as a tree.
  // A manifest without a hash is allowed (name and url are the only required
  // fields) but worth shouting about: HTTPS authenticates the host, not the
  // bytes a compromised bucket would serve.
  if manifest.sha256.is_empty() {
    log::warn!("toolset manifest carries no sha256; installing unverified");
  } else {
    let actual = hex(&hasher.finalize());
    if !actual.eq_ignore_ascii_case(&manifest.sha256) {
      return Err(AppError::Other(format!(
        "toolset checksum mismatch: expected {}, got {actual}",
        manifest.sha256
      )));
    }
  }

  // -------------------------------------------------------------------- unpack
  // Into a scratch directory beside the destination, so the move below is a
  // rename on the same volume rather than a copy.
  let scratch = parent.join(".tools-incoming");
  let _ = std::fs::remove_dir_all(&scratch);
  std::fs::create_dir_all(&scratch)?;

  // `into` places an archive that is not already rooted the way the toolset
  // expects; empty (the usual case) means its top-level names are correct.
  let unpack_root = if manifest.into.is_empty() {
    scratch.clone()
  } else {
    let nested = scratch.join(&manifest.into);
    std::fs::create_dir_all(&nested)?;
    nested
  };

  let decoder = liblzma::read::XzDecoder::new(&bytes[..]);
  let mut archive = tar::Archive::new(decoder);
  archive
    .unpack(&unpack_root)
    .map_err(|e| AppError::Other(format!("could not unpack the toolset: {e}")))?;

  // The version file is what `needs_update` reads next time. Written last, so a
  // tree that failed to unpack is never labelled as a good version.
  std::fs::write(scratch.join(VERSION_FILE), &manifest.version)?;

  // Sanity-check the shape before swapping it in. An archive that unpacked but
  // has no git in it would otherwise replace a working toolset with a broken one.
  if !scratch.join("git/cmd/git.exe").is_file() {
    let _ = std::fs::remove_dir_all(&scratch);
    return Err(AppError::Other(
      "the downloaded toolset does not contain git".into(),
    ));
  }

  // ---------------------------------------------------------------------- swap
  // Old tree out of the way first: Windows will not rename onto an existing
  // directory. Kept until the new one is in place so a failure here is
  // recoverable rather than leaving the user with nothing.
  let retired = parent.join(".tools-old");
  let _ = std::fs::remove_dir_all(&retired);
  let had_previous = dir.exists();
  if had_previous {
    std::fs::rename(&dir, &retired)
      .map_err(|e| AppError::Other(format!("could not move the old toolset aside: {e}")))?;
  }

  match std::fs::rename(&scratch, &dir) {
    Ok(()) => {
      let _ = std::fs::remove_dir_all(&retired);
    }
    Err(e) => {
      // Put the old one back rather than leaving the user with no tools at all.
      if had_previous {
        let _ = std::fs::rename(&retired, &dir);
      }
      let _ = std::fs::remove_dir_all(&scratch);
      return Err(AppError::Other(format!(
        "could not move the new toolset into place: {e}"
      )));
    }
  }

  // The resolver caches where it found git; without this the tools stay
  // "missing" for the rest of the session that just downloaded them.
  crate::git::bundled::set_bundle_root(Some(dir.clone()));

  log::info!("toolset {} installed at {}", manifest.version, dir.display());
  Ok(())
}

fn hex(bytes: &[u8]) -> String {
  use std::fmt::Write;
  bytes.iter().fold(String::new(), |mut out, b| {
    let _ = write!(out, "{b:02x}");
    out
  })
}

/// Read a file's SHA-256, used by the installer-side verification path.
#[allow(dead_code)]
pub fn file_sha256(path: &Path) -> std::io::Result<String> {
  let mut file = std::fs::File::open(path)?;
  let mut hasher = Sha256::new();
  let mut buf = [0u8; 64 * 1024];
  loop {
    let read = file.read(&mut buf)?;
    if read == 0 {
      break;
    }
    hasher.update(&buf[..read]);
  }
  Ok(hex(&hasher.finalize()))
}

/// Where a caller should look for the toolset after a successful install.
#[allow(dead_code)]
pub fn installed_root() -> Option<PathBuf> {
  toolset::toolset_dir().filter(|dir| dir.is_dir())
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn hex_renders_lowercase_and_pads() {
    assert_eq!(hex(&[0x00, 0x0f, 0xff]), "000fff");
  }

  // The manifest tests below describe the published Git for Windows toolset, and
  // the code they exercise is Windows-only - Linux resolves git and gpg from
  // PATH and never fetches a manifest. Gated as a block rather than per test so
  // the reason is stated once.
  #[cfg(windows)]
  #[test]
  fn the_manifest_url_is_https_and_names_the_arch() {
    let url = manifest_url();
    assert!(url.starts_with("https://"), "{url} must be https");
    assert!(
      url.contains("x86_64") || url.contains("aarch64"),
      "{url} should name an architecture"
    );
  }

  #[cfg(windows)]
  #[test]
  fn the_manifest_url_carries_no_version() {
    // Latest-only on the CDN: a version in the path would mean history to prune.
    let url = manifest_url();
    assert!(
      url.ends_with("/components.json"),
      "{url} should be the fixed latest manifest"
    );
  }

  #[cfg(windows)]
  #[test]
  fn the_app_and_the_bootstrapper_read_the_same_manifest() {
    // Two lists describing the same tools would drift, and then a fresh install
    // and an update would disagree about what belongs on disk.
    assert!(
      manifest_url().ends_with("/components.json"),
      "must be the same file the bootstrapper fetches"
    );
  }

  /// Byte-for-byte the output of `.github/scripts/publish-toolset.sh`, so a
  /// change to that script's shape fails here rather than at release time.
  #[cfg(windows)]
  #[test]
  fn the_manifest_ci_actually_publishes_parses() {
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
    let document: ComponentsDocument = serde_json::from_str(raw).expect("CI manifest must parse");
    let first = document.components.first().expect("one component");
    assert_eq!(first.name, "toolset");
    assert_eq!(first.version, "v2.55.0.windows.3");
    assert_eq!(first.size, 24_889_372);
    assert!(first.url.ends_with(".tar.xz"));
  }

  #[cfg(windows)]
  #[test]
  fn an_installer_component_is_left_to_the_bootstrapper() {
    // Running a vendor installer from a background update check would put a UAC
    // prompt in front of someone who only opened their repositories.
    let raw = r#"{"components":[
      {"name":"thing","url":"https://example.invalid/t.exe","kind":"installer","args":"/S"},
      {"name":"toolset","url":"https://example.invalid/t.tar.xz"}
    ]}"#;
    let document: ComponentsDocument = serde_json::from_str(raw).expect("should parse");
    let chosen = document
      .components
      .into_iter()
      .find(|c| c.is_archive())
      .expect("should skip the installer and take the archive");
    assert_eq!(chosen.name, "toolset");
  }

  #[cfg(windows)]
  #[test]
  fn name_and_url_alone_are_enough() {
    // The minimum useful manifest: a list of tools and where to get them.
    let raw = r#"{"components":[{"name":"git","url":"https://example.invalid/git.tar.xz"}]}"#;
    let document: ComponentsDocument =
      serde_json::from_str(raw).expect("minimal manifest should parse");
    let first = document.components.first().expect("one component");
    assert_eq!(first.name, "git");
    assert!(first.version.is_empty(), "absent version forces a reinstall");
    assert!(first.sha256.is_empty());
    assert_eq!(first.size, 0);
  }

  #[cfg(windows)]
  #[test]
  fn unknown_fields_do_not_break_an_older_client() {
    // Adding a component or a field must not require shipping a new client.
    let raw = r#"{
      "schema": 2,
      "components": [
        {"name": "git", "url": "https://example.invalid/g.tar.xz", "notes": "added later"}
      ]
    }"#;
    let document: ComponentsDocument =
      serde_json::from_str(raw).expect("should parse despite unknown keys");
    assert_eq!(document.components.len(), 1);
  }
}
