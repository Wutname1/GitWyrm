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

/// Where the manifest for this architecture lives.
///
/// Latest-only by design: there is no version in the path, so a new toolset
/// replaces the old one and nothing accumulates on the CDN.
fn manifest_url() -> String {
  let arch = if cfg!(target_arch = "aarch64") {
    "aarch64"
  } else {
    "x86_64"
  };
  format!("https://cdn.gitwyrm.com/tools/{arch}/toolset.json")
}

/// What the CDN says the current toolset is.
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize, specta::Type)]
pub struct ToolsetManifest {
  /// Upstream Git for Windows tag, e.g. `v2.55.0.windows.3`.
  pub version: String,
  /// Archive size in bytes, for progress reporting.
  pub size: u64,
  /// SHA-256 of the archive, lowercase hex.
  pub sha256: String,
  /// Absolute URL of the tar.xz.
  pub url: String,
}

/// Fetch the manifest describing the current toolset.
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

  response
    .json::<ToolsetManifest>()
    .await
    .map_err(|e| AppError::Other(format!("could not read toolset manifest: {e}")))
}

/// Whether a download is needed, and what it would install.
///
/// `None` means the toolset on disk is already current. A present tree whose
/// version file does not match is treated as needing an update, and a missing
/// tree needs a first install.
pub async fn needs_update() -> Result<Option<ToolsetManifest>, AppError> {
  let manifest = fetch_manifest().await?;

  if !toolset::is_installed() {
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
  let actual = hex(&hasher.finalize());
  if !actual.eq_ignore_ascii_case(&manifest.sha256) {
    return Err(AppError::Other(format!(
      "toolset checksum mismatch: expected {}, got {actual}",
      manifest.sha256
    )));
  }

  // -------------------------------------------------------------------- unpack
  // Into a scratch directory beside the destination, so the move below is a
  // rename on the same volume rather than a copy.
  let scratch = parent.join(".tools-incoming");
  let _ = std::fs::remove_dir_all(&scratch);
  std::fs::create_dir_all(&scratch)?;

  let decoder = liblzma::read::XzDecoder::new(&bytes[..]);
  let mut archive = tar::Archive::new(decoder);
  archive
    .unpack(&scratch)
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

  #[test]
  fn the_manifest_url_is_https_and_names_the_arch() {
    let url = manifest_url();
    assert!(url.starts_with("https://"), "{url} must be https");
    assert!(
      url.contains("x86_64") || url.contains("aarch64"),
      "{url} should name an architecture"
    );
  }

  #[test]
  fn the_manifest_url_carries_no_version() {
    // Latest-only on the CDN: a version in the path would mean history to prune.
    let url = manifest_url();
    assert!(
      url.ends_with("/toolset.json"),
      "{url} should be the fixed latest manifest"
    );
  }

  #[test]
  fn a_manifest_round_trips_through_json() {
    let raw = r#"{
      "version": "v2.55.0.windows.3",
      "size": 24889372,
      "sha256": "bf4663aa238399c988dc3e3b2ca5ad51bc9e87df462e7bee308dd969e392126f",
      "url": "https://cdn.gitwyrm.com/tools/x86_64/toolset.tar.xz"
    }"#;
    let parsed: ToolsetManifest = serde_json::from_str(raw).expect("manifest should parse");
    assert_eq!(parsed.version, "v2.55.0.windows.3");
    assert_eq!(parsed.size, 24_889_372);
    assert!(parsed.url.ends_with(".tar.xz"));
  }
}
