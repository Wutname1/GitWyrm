//! Signing commits with an SSH key instead of GPG.
//!
//! Git has supported `gpg.format = ssh` since 2.34, and every major host accepts
//! it. For someone who has never signed anything it is the easier of the two:
//! the key is one file, there is no agent or keyring, and most people already
//! have an SSH key sitting in `~/.ssh`.
//!
//! What git needs, all of which this module manages:
//!   gpg.format               = ssh
//!   user.signingkey          = path to the PUBLIC key
//!   gpg.ssh.program          = ssh-keygen (ours, when they have none)
//!   gpg.ssh.allowedSignersFile = who is allowed to sign what
//!
//! That last one is easy to miss and confusing when it is: without it git signs
//! happily but reports "No signature" when reading its own commits back, which
//! reads as signing being broken.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::error::AppError;
use crate::git::bundled;
use crate::git::shell::run_git;

/// An SSH key that can sign commits.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SshKey {
  /// Absolute path to the public half, which is what git wants for
  /// `user.signingkey`.
  pub path: String,
  /// File name, for showing in a list ("id_ed25519.pub").
  pub name: String,
  /// SHA256 fingerprint, the value hosts display next to an uploaded key.
  pub fingerprint: String,
  /// The trailing comment, usually an email or "user@machine".
  pub comment: String,
  /// Key type as ssh-keygen reports it (ED25519, RSA...).
  pub algorithm: String,
}

/// Where the user's SSH keys live.
fn ssh_dir() -> Option<PathBuf> {
  let home = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME"))?;
  Some(PathBuf::from(home).join(".ssh"))
}

/// The ssh-keygen to run: the system one if there is one, else our bundled copy.
///
/// Same fallback-only order as git and gpg, but resolved separately - MinGit
/// ships ssh, ssh-add and ssh-agent yet leaves out ssh-keygen, so the bundled
/// copy is carved from the portable tree and lives beside gpg.
pub fn ssh_keygen_program() -> String {
  bundled::resolve_ssh_keygen().program
}

/// Run ssh-keygen. Unlike gpg, this one takes ordinary Windows paths - it is a
/// native build, not an MSYS one, so no cygdrive translation is involved.
fn run_ssh_keygen(args: &[&str]) -> Result<String, AppError> {
  let program = ssh_keygen_program();
  let mut cmd = Command::new(&program);
  cmd.args(args);

  #[cfg(windows)]
  {
    use std::os::windows::process::CommandExt;
    cmd.creation_flags(crate::git::shell::CREATE_NO_WINDOW);
  }

  let out = cmd.output().map_err(|e| {
    if e.kind() == std::io::ErrorKind::NotFound {
      AppError::Other(format!("No ssh-keygen found at {program}"))
    } else {
      AppError::Io(e)
    }
  })?;

  if !out.status.success() {
    let stderr = String::from_utf8_lossy(&out.stderr);
    return Err(AppError::Other(format!(
      "ssh-keygen failed: {}",
      stderr.trim()
    )));
  }

  Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Every public key in `~/.ssh` that ssh-keygen can read.
///
/// Reads the directory rather than assuming the usual names: people keep
/// per-host keys with arbitrary filenames, and a key GitWyrm cannot see is a
/// key the user has to go to the command line for.
pub fn list_keys() -> Vec<SshKey> {
  let Some(dir) = ssh_dir() else {
    return Vec::new();
  };
  let Ok(entries) = std::fs::read_dir(&dir) else {
    return Vec::new();
  };

  let mut keys: Vec<SshKey> = entries
    .flatten()
    .map(|entry| entry.path())
    .filter(|path| path.extension().is_some_and(|ext| ext == "pub"))
    .filter_map(|path| describe_key(&path))
    .collect();

  // Stable order so the list does not reshuffle between reads.
  keys.sort_by(|a, b| a.name.cmp(&b.name));
  keys
}

/// Ask ssh-keygen to describe one public key.
///
/// Anything it refuses to parse is skipped: `~/.ssh` collects stray files, and
/// one unreadable entry must not hide every other key.
fn describe_key(path: &Path) -> Option<SshKey> {
  let path_str = path.to_string_lossy().replace('\\', "/");
  let raw = run_ssh_keygen(&["-lf", &path_str]).ok()?;
  let (fingerprint, comment, algorithm) = parse_fingerprint_line(raw.trim())?;

  Some(SshKey {
    path: path_str,
    name: path.file_name()?.to_string_lossy().into_owned(),
    fingerprint,
    comment,
    algorithm,
  })
}

/// Pull the parts out of an `ssh-keygen -lf` line.
///
/// Shape: `256 SHA256:<hash> <comment...> (ED25519)`. The comment is whatever
/// sits between the hash and the parenthesised type, and can contain spaces or
/// be absent entirely.
fn parse_fingerprint_line(line: &str) -> Option<(String, String, String)> {
  let mut parts = line.splitn(3, ' ');
  let _bits = parts.next()?;
  let fingerprint = parts.next()?.to_owned();
  let rest = parts.next().unwrap_or("").trim();

  // The algorithm is the last parenthesised token; everything before it is the
  // comment.
  let (comment, algorithm) = match rest.rfind('(') {
    Some(idx) => {
      let algo = rest[idx + 1..].trim_end_matches(')').to_owned();
      (rest[..idx].trim().to_owned(), algo)
    }
    None => (rest.to_owned(), String::new()),
  };

  Some((fingerprint, comment, algorithm))
}

/// Create a new ed25519 signing key in `~/.ssh`.
///
/// ed25519 because it is small, fast, and accepted everywhere; no passphrase
/// for the same reason GPG keys are made without one - a prompt on every commit
/// is what stops people signing at all.
pub fn generate_key(name: &str, comment: &str) -> Result<SshKey, AppError> {
  let comment = comment.trim();
  if comment.is_empty() {
    return Err(AppError::Other(
      "Enter the email to label this key with.".into(),
    ));
  }

  let dir = ssh_dir().ok_or_else(|| {
    AppError::Other("Could not work out where to keep your SSH keys.".into())
  })?;
  std::fs::create_dir_all(&dir)?;

  let stem = sanitize_key_name(name);
  let target = unique_key_path(&dir, &stem);
  let target_str = target.to_string_lossy().replace('\\', "/");

  run_ssh_keygen(&[
    "-t",
    "ed25519",
    "-C",
    comment,
    "-f",
    &target_str,
    // Empty passphrase, and -q so it does not draw its ASCII art randomart.
    "-N",
    "",
    "-q",
  ])?;

  let public = PathBuf::from(format!("{target_str}.pub"));
  describe_key(&public)
    .ok_or_else(|| AppError::Other("The key was made but could not be read back.".into()))
}

/// Turn a display name into something safe to use as a filename.
fn sanitize_key_name(name: &str) -> String {
  let cleaned: String = name
    .trim()
    .chars()
    .map(|c| {
      if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
        c
      } else {
        '_'
      }
    })
    .collect();

  let trimmed = cleaned.trim_matches('_').to_owned();
  if trimmed.is_empty() {
    "gitwyrm_signing".to_owned()
  } else {
    trimmed
  }
}

/// A path that does not collide with a key already there.
///
/// ssh-keygen would otherwise prompt to overwrite, which hangs a non-interactive
/// run, and overwriting someone's existing key is unrecoverable.
fn unique_key_path(dir: &Path, stem: &str) -> PathBuf {
  let first = dir.join(stem);
  if !first.exists() && !dir.join(format!("{stem}.pub")).exists() {
    return first;
  }
  for n in 2..1000 {
    let candidate = dir.join(format!("{stem}_{n}"));
    if !candidate.exists() && !dir.join(format!("{stem}_{n}.pub")).exists() {
      return candidate;
    }
  }
  dir.join(format!("{stem}_new"))
}

/// Delete an SSH key pair.
///
/// Takes the public path (what the UI holds) and removes the private half
/// alongside it. Irreversible; the caller confirms first.
pub fn delete_key(public_path: &str) -> Result<(), AppError> {
  let public = PathBuf::from(public_path);
  if !public.exists() {
    return Err(AppError::Other("That key is already gone.".into()));
  }

  // The private key is the same path without ".pub".
  let private = PathBuf::from(public_path.trim_end_matches(".pub"));

  std::fs::remove_file(&public)?;
  // A missing private half is not an error: half-deleted pairs exist, and the
  // public half going is what matters for signing.
  let _ = std::fs::remove_file(&private);
  Ok(())
}

/// The allowed-signers file GitWyrm manages, next to the keys it describes.
fn allowed_signers_path() -> Option<PathBuf> {
  Some(ssh_dir()?.join("allowed_signers"))
}

/// Record a key as a legitimate signer for an email.
///
/// Without this git signs but cannot verify, reporting "No signature" on the
/// user's own commits - which looks exactly like signing having failed. Each
/// line is `<email> <key type> <key data>`.
fn add_to_allowed_signers(email: &str, public_path: &str) -> Result<PathBuf, AppError> {
  let path = allowed_signers_path()
    .ok_or_else(|| AppError::Other("Could not work out where to keep your SSH keys.".into()))?;

  let key_line = std::fs::read_to_string(public_path)?.trim().to_owned();
  if key_line.is_empty() {
    return Err(AppError::Other("That public key file is empty.".into()));
  }
  let entry = format!("{} {}", email.trim(), key_line);

  let existing = std::fs::read_to_string(&path).unwrap_or_default();
  // The key half identifies the entry; the same key under a second email is a
  // different, legitimate line.
  if existing.lines().any(|line| line.trim() == entry) {
    return Ok(path);
  }

  let mut updated = existing;
  if !updated.is_empty() && !updated.ends_with('\n') {
    updated.push('\n');
  }
  updated.push_str(&entry);
  updated.push('\n');

  if let Some(parent) = path.parent() {
    std::fs::create_dir_all(parent)?;
  }
  std::fs::write(&path, updated)?;
  Ok(path)
}

/// Point a repository at an SSH key for signing, and switch it to SSH mode.
pub fn enable_ssh_signing(repo_path: &str, public_path: &str, email: &str) -> Result<(), AppError> {
  let signers = add_to_allowed_signers(email, public_path)?;
  let signers_str = signers.to_string_lossy().replace('\\', "/");

  run_git(Some(repo_path), &["config", "gpg.format", "ssh"])?;
  run_git(Some(repo_path), &["config", "user.signingkey", public_path])?;
  run_git(
    Some(repo_path),
    &["config", "gpg.ssh.allowedSignersFile", &signers_str],
  )?;
  // Only pin the program when it is ours; a system ssh-keygen is already on
  // PATH and hardcoding a path there would break if they move their install.
  if bundled::resolve_ssh_keygen().source == bundled::ToolSource::Bundled {
    run_git(
      Some(repo_path),
      &["config", "gpg.ssh.program", &ssh_keygen_program()],
    )?;
  }
  run_git(Some(repo_path), &["config", "commit.gpgsign", "true"])?;
  Ok(())
}

/// Switch a repository back to GPG signing.
///
/// `gpg.format` is unset rather than set to "openpgp" so the repository follows
/// whatever the user's global default is, which is what it did before SSH was
/// ever turned on.
pub fn use_gpg_format(repo_path: &str) -> Result<(), AppError> {
  let _ = run_git(Some(repo_path), &["config", "--unset", "gpg.format"]);
  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn parses_a_normal_fingerprint_line() {
    let line = "256 SHA256:qLeJiGJIv//KpJHog/ol5H t@e.invalid (ED25519)";
    let (fp, comment, algo) = parse_fingerprint_line(line).unwrap();
    assert_eq!(fp, "SHA256:qLeJiGJIv//KpJHog/ol5H");
    assert_eq!(comment, "t@e.invalid");
    assert_eq!(algo, "ED25519");
  }

  #[test]
  fn parses_a_comment_containing_spaces() {
    // ssh-keygen -C takes arbitrary text, and "Jane's laptop" is common.
    let line = "256 SHA256:abc Jane Doe work laptop (ED25519)";
    let (_, comment, algo) = parse_fingerprint_line(line).unwrap();
    assert_eq!(comment, "Jane Doe work laptop");
    assert_eq!(algo, "ED25519");
  }

  #[test]
  fn parses_a_line_with_no_comment() {
    // A key generated without -C reports "no comment".
    let line = "3072 SHA256:xyz no comment (RSA)";
    let (fp, _, algo) = parse_fingerprint_line(line).unwrap();
    assert_eq!(fp, "SHA256:xyz");
    assert_eq!(algo, "RSA");
  }

  #[test]
  fn a_malformed_line_is_rejected_rather_than_panicking() {
    // Garbage in ~/.ssh must skip that file, not take down the whole listing.
    assert!(parse_fingerprint_line("").is_none());
    assert!(parse_fingerprint_line("nonsense").is_none());
  }

  #[test]
  fn key_names_become_safe_filenames() {
    assert_eq!(sanitize_key_name("work laptop"), "work_laptop");
    // Path separators and dots all become underscores, so nothing can escape
    // ~/.ssh via a crafted name.
    assert_eq!(sanitize_key_name("gitwyrm/../etc"), "gitwyrm____etc");
    assert_eq!(sanitize_key_name("  "), "gitwyrm_signing");
    // A name that sanitizes to nothing but underscores must not yield "".
    assert_eq!(sanitize_key_name("///"), "gitwyrm_signing");
  }

  #[test]
  fn generating_requires_a_comment_to_label_the_key() {
    assert!(generate_key("work", "").is_err());
    assert!(generate_key("work", "   ").is_err());
  }

  #[test]
  fn a_unique_path_is_chosen_when_one_is_taken() {
    let dir = tempfile::tempdir().unwrap();
    let first = unique_key_path(dir.path(), "key");
    assert_eq!(first.file_name().unwrap(), "key");

    // Simulate the pair already existing; the next call must not collide.
    std::fs::write(dir.path().join("key"), "x").unwrap();
    std::fs::write(dir.path().join("key.pub"), "x").unwrap();
    let second = unique_key_path(dir.path(), "key");
    assert_eq!(second.file_name().unwrap(), "key_2");
  }

  #[test]
  fn deleting_a_missing_key_is_an_error_not_a_panic() {
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("nope.pub");
    assert!(delete_key(&missing.to_string_lossy()).is_err());
  }
}
