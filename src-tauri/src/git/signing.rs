//! Commit signing support: locating gpg, making a relocated gpg usable, and
//! reporting what the user's signing setup currently looks like.
//!
//! Key generation and the "set this up for me" flow live in the Security
//! settings screen; this module is the plumbing underneath it.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::RwLock;

use crate::error::AppError;
use crate::git::bundled::{self, ToolSource};
use crate::git::shell::run_git;

/// The gpg path the user set in Settings, if any.
static GPG_PROGRAM: RwLock<Option<String>> = RwLock::new(None);

/// Set the gpg program. Empty clears it, so resolution falls back to PATH then
/// the bundled copy.
pub fn set_gpg_program(path: Option<&str>) {
  let cleaned = path
    .map(str::trim)
    .filter(|s| !s.is_empty())
    .map(str::to_owned);
  if let Ok(mut guard) = GPG_PROGRAM.write() {
    *guard = cleaned;
  }
  bundled::clear_gpg_cache();
}

fn configured_gpg() -> Option<String> {
  GPG_PROGRAM.read().ok().and_then(|g| g.clone())
}

pub fn gpg_program_name() -> String {
  bundled::resolve_gpg(configured_gpg().as_deref()).program
}

pub fn gpg_source() -> ToolSource {
  bundled::resolve_gpg(configured_gpg().as_deref()).source
}

/// Run a gpg subcommand, capturing output. Errors carry gpg's stderr, which is
/// where it explains itself.
pub fn run_gpg(args: &[&str]) -> Result<String, AppError> {
  let program = gpg_program_name();
  let mut cmd = Command::new(&program);
  cmd.args(args);

  // A relocated gpg cannot find its own agent without being told where it is,
  // so every invocation carries the same environment the setup step wrote.
  apply_gpg_env(&mut cmd);

  #[cfg(windows)]
  {
    use std::os::windows::process::CommandExt;
    cmd.creation_flags(crate::git::shell::CREATE_NO_WINDOW);
  }

  let out = cmd.output().map_err(|e| {
    if e.kind() == std::io::ErrorKind::NotFound {
      AppError::Other(format!("No gpg found at {program}"))
    } else {
      AppError::Io(e)
    }
  })?;

  if !out.status.success() {
    let stderr = String::from_utf8_lossy(&out.stderr);
    return Err(AppError::Other(format!(
      "gpg {} failed: {}",
      args.first().unwrap_or(&""),
      stderr.trim()
    )));
  }

  Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Point a gpg invocation at the home directory we manage.
///
/// Only set when we are running the bundled gpg. A system gpg must keep using
/// the user's real `~/.gnupg` - redirecting it would hide the keys they
/// already have.
fn apply_gpg_env(cmd: &mut Command) {
  if gpg_source() != ToolSource::Bundled {
    return;
  }
  if let Some(home) = managed_gnupg_home() {
    cmd.env("GNUPGHOME", to_cygdrive(&home));
  }
}

/// Convert a Windows path to the `/cygdrive/c/...` form GnuPG expects.
///
/// The GnuPG that ships with Git for Windows is an MSYS build, so it resolves
/// paths through a POSIX layer rather than the Win32 API. Handed a native path
/// it prefixes its own root and looks in the wrong place entirely - the failure
/// reads as `failed to create temporary file '/cygdrive/c/.../C:\...'`, then
/// "No agent running", which points nowhere near the real cause. Running under
/// Git Bash hides this, because the MSYS runtime is already initialised there;
/// the app spawns gpg directly, so it has to hand over a path gpg can parse.
fn to_cygdrive(path: &Path) -> String {
  let text = path.to_string_lossy().replace('\\', "/");

  // "C:/Users/..." -> "/cygdrive/c/Users/..."
  let bytes = text.as_bytes();
  if bytes.len() >= 2 && bytes[1] == b':' && (bytes[0] as char).is_ascii_alphabetic() {
    let drive = (bytes[0] as char).to_ascii_lowercase();
    let rest = text[2..].trim_start_matches('/');
    return format!("/cygdrive/{drive}/{rest}");
  }

  text
}

/// The GNUPGHOME used with the bundled gpg.
///
/// Deliberately short and directly under the local app data root. gpg-agent
/// opens unix-style sockets inside this directory and silently refuses to
/// start when the resulting socket path is too long ("socket name ... is too
/// long"), which surfaces to the user as the useless "No agent running".
fn managed_gnupg_home() -> Option<PathBuf> {
  let base = dirs_local_app_data()?;
  Some(base.join("GitWyrm").join("gnupg"))
}

/// `%LOCALAPPDATA%`, the shortest stable per-user writable root on Windows.
fn dirs_local_app_data() -> Option<PathBuf> {
  std::env::var_os("LOCALAPPDATA").map(PathBuf::from)
}

/// Write the config that makes a relocated gpg work.
///
/// `gpg.exe` has `/usr/bin/gpg-agent` compiled in as the agent path. That is an
/// MSYS path which does not exist once the binaries are copied out of a Git for
/// Windows install, so every operation needing the agent - including key
/// generation and signing - fails with "No agent running". Writing an explicit
/// `agent-program` (and a matching `pinentry-program` for the agent) is what
/// makes the bundled copy functional at all.
///
/// Only touches the home directory we manage, never the user's own `~/.gnupg`.
pub fn ensure_bundled_gpg_configured() -> Result<(), AppError> {
  if gpg_source() != ToolSource::Bundled {
    return Ok(());
  }

  let Some(home) = managed_gnupg_home() else {
    return Err(AppError::Other(
      "Could not work out where to keep your signing keys.".into(),
    ));
  };
  std::fs::create_dir_all(&home)?;

  let program = PathBuf::from(gpg_program_name());
  let dir = program.parent().unwrap_or(Path::new("."));

  let agent = dir.join("gpg-agent.exe");
  if agent.is_file() {
    write_conf_line(
      &home.join("gpg.conf"),
      "agent-program",
      &agent.to_string_lossy(),
    )?;
  }

  // pinentry is what asks for a passphrase. The w32 build draws a native
  // dialog; the plain one wants a console we do not have.
  let pinentry = ["pinentry-w32.exe", "pinentry.exe"]
    .iter()
    .map(|name| dir.join(name))
    .find(|p| p.is_file());
  if let Some(pinentry) = pinentry {
    write_conf_line(
      &home.join("gpg-agent.conf"),
      "pinentry-program",
      &pinentry.to_string_lossy(),
    )?;
  }

  Ok(())
}

/// Set a single `key value` directive in a gpg config file, replacing any
/// existing line for that key and leaving everything else untouched.
fn write_conf_line(path: &Path, key: &str, value: &str) -> Result<(), AppError> {
  // Strip the extended-length prefix before anything else. Rust hands back
  // `\\?\C:\...` from canonicalize and from Tauri's path APIs, and the
  // backslash rewrite below would turn it into `//?/C:/...`, which gpg reads as
  // a UNC host named `?`. It then fails to find its agent and reports the
  // useless "No agent running", or - once keyboxd is involved - refuses to open
  // the keyring at all.
  let value = value.strip_prefix(r"\\?\").unwrap_or(value);

  // gpg wants forward slashes even on Windows; a backslash reads as an escape.
  let value = value.replace('\\', "/");

  let existing = std::fs::read_to_string(path).unwrap_or_default();
  let mut lines: Vec<String> = existing
    .lines()
    .filter(|line| !line.trim_start().starts_with(key))
    .map(str::to_owned)
    .collect();
  lines.push(format!("{key} {value}"));

  std::fs::write(path, format!("{}\n", lines.join("\n")))?;
  Ok(())
}

/// A signing key gpg knows about.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SigningKey {
  /// Long key id, the value git wants for `user.signingkey`.
  pub id: String,
  /// Full fingerprint, shown to the user for verification.
  pub fingerprint: String,
  /// "Real Name <email>" as recorded on the key.
  pub uid: String,
}

/// List secret (signing-capable) keys gpg can see.
///
/// Parses `--with-colons`, gpg's stable machine-readable format. The
/// human-readable output is explicitly not a stable interface.
pub fn list_secret_keys() -> Result<Vec<SigningKey>, AppError> {
  let raw = run_gpg(&["--list-secret-keys", "--with-colons", "--fingerprint"])?;
  Ok(parse_secret_keys(&raw))
}

/// Pull keys out of gpg's colon-delimited listing.
///
/// Records arrive in order: an `sec` line opens a key, the `fpr` after it
/// carries the fingerprint, and the `uid` after that carries the identity.
/// Split out from the gpg call so it can be tested against real output.
fn parse_secret_keys(raw: &str) -> Vec<SigningKey> {
  let mut keys: Vec<SigningKey> = Vec::new();
  let mut pending_id: Option<String> = None;

  for line in raw.lines() {
    let fields: Vec<&str> = line.split(':').collect();
    match fields.first().copied() {
      Some("sec") => {
        // Field 5 is the long key id.
        pending_id = fields.get(4).map(|s| (*s).to_owned());
      }
      Some("fpr") => {
        // Field 10 is the fingerprint. Only the one right after `sec` matters;
        // subkeys emit their own and would otherwise overwrite it.
        if let Some(id) = pending_id.take() {
          keys.push(SigningKey {
            id,
            fingerprint: fields.get(9).map(|s| (*s).to_owned()).unwrap_or_default(),
            uid: String::new(),
          });
        }
      }
      Some("uid") => {
        // Field 10 is the user id. Attach to the most recent key that lacks one.
        if let Some(last) = keys.last_mut() {
          if last.uid.is_empty() {
            last.uid = fields.get(9).map(|s| (*s).to_owned()).unwrap_or_default();
          }
        }
      }
      _ => {}
    }
  }

  keys
}

/// What the user's signing setup looks like right now, for the Security screen.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SigningStatus {
  /// Whether a gpg was found at all.
  pub gpg_available: bool,
  /// Which gpg is in use.
  pub gpg_source: ToolSource,
  /// gpg's version banner, when it answered.
  pub gpg_version: Option<String>,
  /// Signing keys gpg can see.
  pub keys: Vec<SigningKey>,
  /// Whether this repository signs commits by default.
  pub signing_enabled: bool,
  /// The key this repository is set to sign with.
  pub configured_key: Option<String>,
  /// Set when the user's git config has a `gpg.format` git refuses to accept.
  /// Git hard-fails every commit in this state, so the UI offers to repair it.
  pub broken_format: Option<String>,
  /// True when `user.signingkey` is present but empty. Git treats blank as a
  /// real value and gpg rejects it, so signed commits fail until it is cleared.
  /// Repaired by the same button as `broken_format`.
  pub blank_signing_key: bool,
}

/// Read the user's current signing configuration.
pub fn signing_status(repo_path: &str) -> SigningStatus {
  let version = run_gpg(&["--version"])
    .ok()
    .and_then(|out| out.lines().next().map(str::to_owned));

  let keys = list_secret_keys().unwrap_or_default();

  let configured_key = git_config_value(repo_path, "user.signingkey");
  let signing_enabled = git_config_value(repo_path, "commit.gpgsign")
    .map(|v| v.eq_ignore_ascii_case("true"))
    .unwrap_or(false);

  SigningStatus {
    gpg_available: version.is_some(),
    gpg_source: gpg_source(),
    gpg_version: version,
    keys,
    signing_enabled,
    configured_key,
    broken_format: detect_broken_format(repo_path),
    blank_signing_key: has_blank_signing_key(repo_path),
  }
}

/// Whether `user.signingkey` is set to an empty string somewhere.
///
/// Blank is not the same as absent: git treats it as a real value, so it wins
/// over selecting a key by email, and hands the empty string to gpg, which
/// answers `skipped "": Invalid user ID` and aborts the commit. Signing is then
/// broken for the repository with an error that names neither the setting nor
/// the scope holding it.
///
/// Reported so the Security screen can offer the same one-click repair it does
/// for a broken `gpg.format`.
fn has_blank_signing_key(repo_path: &str) -> bool {
  run_git(Some(repo_path), &["config", "--get", "user.signingkey"])
    .is_ok_and(|out| out.stdout.trim().is_empty())
}

/// Read one git config value, treating "unset" and "failed" alike as None.
fn git_config_value(repo_path: &str, key: &str) -> Option<String> {
  let out = run_git(Some(repo_path), &["config", "--get", key]).ok()?;
  let value = out.stdout.trim();
  (!value.is_empty()).then(|| value.to_owned())
}

/// Detect a `gpg.format` set to something git rejects.
///
/// An empty `gpg.format =` in a global config makes git abort *every* commit
/// with "error: invalid value for 'gpg.format'", whether or not signing is on.
/// It is easy to end up with and impossible to diagnose from the error, so the
/// Security screen offers a one-click repair.
fn detect_broken_format(repo_path: &str) -> Option<String> {
  let out = run_git(Some(repo_path), &["config", "--get", "gpg.format"]).ok()?;
  let value = out.stdout.trim();

  // A missing key exits non-zero and lands in the `ok()?` above, so reaching
  // here means the key is present. Present-but-blank is the broken case that
  // makes git refuse to commit; anything git accepts is fine.
  if matches!(value, "openpgp" | "ssh" | "x509") {
    None
  } else {
    Some(value.to_owned())
  }
}

/// Create a signing key and return its long id.
///
/// Uses `--quick-generate-key`, which is gpg's unattended path: no prompts, no
/// interactive menu. ed25519 is the modern default - small, fast, and accepted
/// by every major host.
///
/// The key is created with no passphrase. That is a deliberate trade for the
/// people this feature is for: a passphrase means a pinentry dialog on every
/// single commit, which is exactly the friction that stops beginners signing at
/// all. The key lives in the user's own profile with the same protection as
/// their stored git credentials.
pub fn generate_key(name: &str, email: &str) -> Result<String, AppError> {
  let name = name.trim();
  let email = email.trim();
  if name.is_empty() || email.is_empty() {
    return Err(AppError::Other(
      "Enter your name and email before making a key.".into(),
    ));
  }
  // gpg parses "Name <email>", so a stray angle bracket would corrupt the uid.
  if name.contains(['<', '>']) || email.contains(['<', '>']) {
    return Err(AppError::Other(
      "Your name and email cannot contain < or > characters.".into(),
    ));
  }

  ensure_bundled_gpg_configured()?;

  // Which keys existed beforehand, so the new one can be identified by
  // subtraction. gpg does not print the id of the key it just made, and its
  // listing order follows keyring position rather than age - so "the last
  // match" picks whichever key gpg happens to list last, which for someone
  // with two keys on one address is the old one.
  let before: std::collections::HashSet<String> = list_secret_keys()
    .unwrap_or_default()
    .into_iter()
    .map(|k| k.id)
    .collect();

  let uid = format!("{name} <{email}>");
  run_gpg(&[
    "--batch",
    "--passphrase",
    "",
    "--quick-generate-key",
    &uid,
    "ed25519",
    "sign",
    "never",
  ])
  .map_err(|e| {
    // gpg refuses a duplicate identity. Say what to do about it rather than
    // passing through a message that reads like a crash.
    if e.to_string().contains("already exists") {
      AppError::Other(format!(
        "You already have a signing key for {email}. Pick it from the list, or use a different email."
      ))
    } else {
      e
    }
  })?;

  let after = list_secret_keys()?;
  after
    .into_iter()
    .find(|k| !before.contains(&k.id))
    .map(|k| k.id)
    .ok_or_else(|| AppError::Other("The key was made but could not be found again.".into()))
}

/// Delete a signing key, secret half included.
///
/// Irreversible: the secret key is what signs, and gpg keeps no copy once it is
/// gone. Anything already signed with it stays valid and verifiable by anyone
/// holding the public half, but nothing new can ever be signed with it again.
/// The caller is expected to have confirmed with the user first.
///
/// `--delete-secret-and-public-key` takes the fingerprint, not the key id: an
/// id is only the tail of a fingerprint and can collide, and gpg refuses a
/// non-unique specification rather than guessing.
pub fn delete_key(fingerprint: &str) -> Result<(), AppError> {
  let fingerprint = fingerprint.trim();
  if fingerprint.is_empty() {
    return Err(AppError::Other("No key was given to delete.".into()));
  }

  run_gpg(&[
    "--batch",
    "--yes",
    "--delete-secret-and-public-key",
    fingerprint,
  ])?;
  Ok(())
}

/// Clear a repository's signing config when it points at a key that is gone.
///
/// Without this, deleting the key a repository was signing with leaves git
/// configured to sign with a key that no longer exists, and every commit fails
/// with "secret key not available".
pub fn forget_key_if_configured(repo_path: &str, key_id: &str) -> Result<(), AppError> {
  let configured = git_config_value(repo_path, "user.signingkey");
  let points_at_it = configured
    .as_deref()
    .is_some_and(|c| c.eq_ignore_ascii_case(key_id) || key_id.ends_with(c) || c.ends_with(key_id));

  if points_at_it {
    let _ = run_git(Some(repo_path), &["config", "--unset", "user.signingkey"]);
    let _ = run_git(Some(repo_path), &["config", "commit.gpgsign", "false"]);
  }
  Ok(())
}

/// Export a public key in the armored form hosting providers expect.
pub fn export_public_key(key_id: &str) -> Result<String, AppError> {
  run_gpg(&["--armor", "--export", key_id])
}

/// Turn on signing for a repository with the given key.
pub fn enable_signing(repo_path: &str, key_id: &str) -> Result<(), AppError> {
  ensure_bundled_gpg_configured()?;

  run_git(Some(repo_path), &["config", "user.signingkey", key_id])?;
  run_git(Some(repo_path), &["config", "commit.gpgsign", "true"])?;
  // Signing with our gpg means git has to be told where it is; a bare `gpg`
  // would not resolve on a machine that has none installed.
  run_git(
    Some(repo_path),
    &["config", "gpg.program", &gpg_program_name()],
  )?;
  Ok(())
}

/// Turn off signing for a repository, leaving the key in place.
pub fn disable_signing(repo_path: &str) -> Result<(), AppError> {
  run_git(Some(repo_path), &["config", "commit.gpgsign", "false"])?;
  Ok(())
}

/// Remove a `gpg.format` that git refuses to accept.
///
/// Unsetting restores git's default (openpgp) rather than guessing at what the
/// user meant. Walks every scope because the bad value is usually global, not
/// in the repository the user happens to have open.
pub fn repair_gpg_format(repo_path: &str) -> Result<(), AppError> {
  for scope in ["--local", "--global", "--system"] {
    // Absent in a scope is the normal case and exits non-zero; only the scopes
    // that actually carry the value need clearing.
    let _ = run_git(Some(repo_path), &["config", scope, "--unset-all", "gpg.format"]);
  }
  clear_blank_signing_keys(repo_path);
  Ok(())
}

/// Removes `user.signingkey` from any scope where it is set but empty.
///
/// An empty value is a real entry to git, not an absent one. It takes
/// precedence over the email-based key selection that would otherwise work, and
/// git passes the blank straight to gpg, which refuses it:
///
/// ```text
/// gpg: skipped "": Invalid user ID
/// fatal: failed to write commit object
/// ```
///
/// Every signed commit in the repository fails until it is cleared, and the
/// message names neither the setting nor the scope holding it, so it is close
/// to undiagnosable from the error alone.
///
/// Only ever removes a value that is already useless, so it cannot discard a
/// key anyone meant to keep. Best effort per scope: `--system` is usually not
/// writable and simply exits non-zero.
pub fn clear_blank_signing_keys(repo_path: &str) {
  for scope in ["--local", "--global", "--system"] {
    let value = run_git(
      Some(repo_path),
      &["config", scope, "--get", "user.signingkey"],
    );
    // Non-zero means absent in this scope, which is the state we want anyway.
    let Ok(out) = value else { continue };
    if out.stdout.trim().is_empty() {
      let _ = run_git(
        Some(repo_path),
        &["config", scope, "--unset-all", "user.signingkey"],
      );
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  // Real `gpg --list-secret-keys --with-colons` output, trimmed to the records
  // that matter. Captured from GnuPG 2.4.7.
  const COLON_LISTING: &str = "\
sec:u:255:22:AF9457A4B4D1E25F:1753424000:::u:::scESC:::+:::23::0:
fpr:::::::::54D565B5C9032B40569CAA82AF9457A4B4D1E25F:
grp:::::::::A1B2C3D4E5F60718293A4B5C6D7E8F9012345678:
uid:u::::1753424000::1234567890ABCDEF::Test User <test@example.com>::::::::::0:
ssb:u:255:18:0123456789ABCDEF:1753424000::::::e:::+:::23:
fpr:::::::::FEDCBA9876543210FEDCBA9876543210FEDCBA98:";

  #[test]
  fn parses_key_id_fingerprint_and_uid() {
    let keys = parse_secret_keys(COLON_LISTING);
    assert_eq!(keys.len(), 1);
    assert_eq!(keys[0].id, "AF9457A4B4D1E25F");
    assert_eq!(
      keys[0].fingerprint,
      "54D565B5C9032B40569CAA82AF9457A4B4D1E25F"
    );
    assert_eq!(keys[0].uid, "Test User <test@example.com>");
  }

  #[test]
  fn a_subkey_fingerprint_does_not_overwrite_the_primary() {
    // The listing ends with an ssb + its own fpr. Without the take() guard that
    // trailing fingerprint would replace the primary key's.
    let keys = parse_secret_keys(COLON_LISTING);
    assert_eq!(keys.len(), 1);
    assert!(keys[0].fingerprint.starts_with("54D565B5"));
  }

  #[test]
  fn an_empty_listing_yields_no_keys() {
    assert!(parse_secret_keys("").is_empty());
    assert!(parse_secret_keys("gpg: checking the trustdb\n").is_empty());
  }

  /// A repository whose config is sealed off from the machine's real one.
  ///
  /// These tests read and clear `user.signingkey` in every scope, so without
  /// this they would both consult and *modify* the developer's own global git
  /// config -- clearing a real setting, and failing or passing depending on what
  /// that machine happens to have. `GIT_CONFIG_GLOBAL` and `GIT_CONFIG_SYSTEM`
  /// point both scopes at throwaway files.
  ///
  /// Env vars are process-wide and Rust runs tests in threads, so the guard also
  /// serialises: two of these running at once would trade config paths.
  struct IsolatedRepo {
    dir: tempfile::TempDir,
    _lock: std::sync::MutexGuard<'static, ()>,
  }

  static CONFIG_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

  impl IsolatedRepo {
    fn new() -> Self {
      let lock = CONFIG_LOCK.lock().unwrap_or_else(|e| e.into_inner());
      let dir = tempfile::tempdir().unwrap();
      std::env::set_var("GIT_CONFIG_GLOBAL", dir.path().join("gitconfig-global"));
      std::env::set_var("GIT_CONFIG_SYSTEM", dir.path().join("gitconfig-system"));
      let repo = Self { dir, _lock: lock };
      run_git(Some(repo.path()), &["init", "-q", "."]).unwrap();
      repo
    }

    fn path(&self) -> &str {
      self.dir.path().to_str().unwrap()
    }

    fn set_local(&self, key: &str, value: &str) {
      run_git(Some(self.path()), &["config", "--local", key, value]).unwrap();
    }

    fn local(&self, key: &str) -> Option<String> {
      run_git(Some(self.path()), &["config", "--local", "--get", key])
        .ok()
        .map(|o| o.stdout.trim().to_owned())
        .filter(|v| !v.is_empty())
    }
  }

  impl Drop for IsolatedRepo {
    fn drop(&mut self) {
      std::env::remove_var("GIT_CONFIG_GLOBAL");
      std::env::remove_var("GIT_CONFIG_SYSTEM");
    }
  }

  #[test]
  fn blank_signing_key_is_detected() {
    let repo = IsolatedRepo::new();
    repo.set_local("user.signingkey", "");
    assert!(has_blank_signing_key(repo.path()));
  }

  #[test]
  fn clearing_removes_the_blank_key() {
    let repo = IsolatedRepo::new();
    repo.set_local("user.signingkey", "");

    clear_blank_signing_keys(repo.path());

    assert!(!has_blank_signing_key(repo.path()));
    // Absent, not merely empty: absent is the state that lets git fall back to
    // choosing a key by email, which is the whole point of clearing it.
    assert!(repo.local("user.signingkey").is_none());
  }

  /// The repair must never throw away a key someone meant to keep -- it only
  /// ever removes a value that is already useless.
  #[test]
  fn clearing_leaves_a_real_key_alone() {
    let repo = IsolatedRepo::new();
    repo.set_local("user.signingkey", "1C7195BAAB12CF6A");

    clear_blank_signing_keys(repo.path());

    assert_eq!(repo.local("user.signingkey").as_deref(), Some("1C7195BAAB12CF6A"));
    assert!(!has_blank_signing_key(repo.path()));
  }

  #[test]
  fn a_repo_with_no_signing_key_is_not_flagged() {
    let repo = IsolatedRepo::new();
    assert!(!has_blank_signing_key(repo.path()));
  }

  #[test]
  fn write_conf_line_replaces_rather_than_appends() {
    let dir = tempfile::tempdir().unwrap();
    let conf = dir.path().join("gpg.conf");

    write_conf_line(&conf, "agent-program", "C:/one/gpg-agent.exe").unwrap();
    write_conf_line(&conf, "agent-program", "C:/two/gpg-agent.exe").unwrap();

    let body = std::fs::read_to_string(&conf).unwrap();
    assert_eq!(body.matches("agent-program").count(), 1);
    assert!(body.contains("C:/two/gpg-agent.exe"));
  }

  #[test]
  fn write_conf_line_keeps_unrelated_directives() {
    let dir = tempfile::tempdir().unwrap();
    let conf = dir.path().join("gpg.conf");
    std::fs::write(&conf, "use-agent\ncharset utf-8\n").unwrap();

    write_conf_line(&conf, "agent-program", "C:/gpg-agent.exe").unwrap();

    let body = std::fs::read_to_string(&conf).unwrap();
    assert!(body.contains("use-agent"));
    assert!(body.contains("charset utf-8"));
    assert!(body.contains("agent-program C:/gpg-agent.exe"));
  }

  #[test]
  fn windows_paths_become_cygdrive_paths() {
    // The exact shape verified against GnuPG 2.4.9 outside Git Bash: a native
    // path is rejected, this form works.
    assert_eq!(
      to_cygdrive(Path::new(r"C:\Users\me\AppData\Local\GitWyrm\gnupg")),
      "/cygdrive/c/Users/me/AppData/Local/GitWyrm/gnupg"
    );
  }

  #[test]
  fn cygdrive_conversion_lowercases_the_drive_letter() {
    assert_eq!(to_cygdrive(Path::new(r"D:\keys")), "/cygdrive/d/keys");
  }

  #[test]
  fn paths_that_are_not_drive_letters_pass_through() {
    // A UNC or already-POSIX path has no drive letter to rewrite.
    assert_eq!(to_cygdrive(Path::new("/tmp/keys")), "/tmp/keys");
  }

  #[test]
  fn deleting_requires_a_key_to_delete() {
    // Reached before any gpg call. A blank fingerprint passed through to
    // `--delete-secret-and-public-key` would match nothing at best, and is not
    // a request worth sending.
    assert!(delete_key("").is_err());
    assert!(delete_key("   ").is_err());
  }

  #[test]
  fn key_generation_rejects_blank_identity() {
    // Reached before any gpg call, so this is safe to assert without gpg present.
    assert!(generate_key("", "a@b.com").is_err());
    assert!(generate_key("Name", "   ").is_err());
  }

  #[test]
  fn key_generation_rejects_angle_brackets_in_the_identity() {
    // gpg parses "Name <email>"; an embedded bracket would corrupt the uid.
    assert!(generate_key("Ev<il", "a@b.com").is_err());
    assert!(generate_key("Name", "a@b.com>extra").is_err());
  }

  #[test]
  fn the_extended_length_prefix_is_stripped() {
    // Rust's canonicalize and Tauri's path APIs both return `\\?\C:\...`.
    // Rewriting its backslashes would produce `//?/C:/...`, which gpg treats as
    // a UNC host called `?` - it then cannot find its agent or its keyring.
    let dir = tempfile::tempdir().unwrap();
    let conf = dir.path().join("gpg.conf");

    write_conf_line(&conf, "agent-program", r"\\?\C:\tools\gpg\gpg-agent.exe").unwrap();

    let body = std::fs::read_to_string(&conf).unwrap();
    assert!(
      body.contains("C:/tools/gpg/gpg-agent.exe"),
      "expected a plain drive path, got: {body}"
    );
    assert!(!body.contains("//?/"), "UNC-looking path survived: {body}");
    assert!(!body.contains('?'), "prefix remnant survived: {body}");
  }

  #[test]
  fn a_real_unc_path_is_left_alone() {
    // `\\server\share` is a genuine network path, not the extended-length
    // prefix, and gpg handles it once the slashes are flipped.
    let dir = tempfile::tempdir().unwrap();
    let conf = dir.path().join("gpg.conf");

    write_conf_line(&conf, "agent-program", r"\\server\share\gpg-agent.exe").unwrap();

    let body = std::fs::read_to_string(&conf).unwrap();
    assert!(
      body.contains("//server/share/gpg-agent.exe"),
      "a real UNC path should survive: {body}"
    );
  }

  #[test]
  fn backslashes_become_forward_slashes() {
    // gpg treats a backslash as an escape character, so a Windows path written
    // verbatim silently fails to resolve.
    let dir = tempfile::tempdir().unwrap();
    let conf = dir.path().join("gpg.conf");

    write_conf_line(&conf, "agent-program", r"C:\Program Files\gpg-agent.exe").unwrap();

    let body = std::fs::read_to_string(&conf).unwrap();
    assert!(body.contains("C:/Program Files/gpg-agent.exe"));
    assert!(!body.contains('\\'));
  }
}
