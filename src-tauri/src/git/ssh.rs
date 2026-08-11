//! Connecting to a git host over SSH: the keys in `~/.ssh`, which key a host
//! actually uses, and whether that key is accepted.
//!
//! SSH here is the transport - `git@github.com:owner/repo.git` - not commit
//! signing. Setting it up normally means generating a key, editing
//! `~/.ssh/config`, and reading `Permission denied (publickey)` until it works.
//! The failure this exists to catch: a `Host *` rule pointing every host at one
//! key, while the key the host actually accepts is a different one. Git only
//! ever says "Permission denied", never which key it offered.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::error::AppError;
use crate::git::bundled;

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

/* ------------------------------------------------- connecting to a host */

/// What happened when we tried to reach a host over SSH.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SshTestResult {
  /// True when the host recognised the key.
  pub ok: bool,
  /// Plain-language outcome for the user.
  pub message: String,
  /// Who the host said we are, when it said so ("Wutname1" on GitHub).
  pub identity: Option<String>,
  /// A key in ~/.ssh that this host WOULD accept, when the current one failed.
  /// This is the whole point: git only ever reports "Permission denied", never
  /// that a different key on the same machine would have worked.
  pub working_key: Option<String>,
}

/// Try to authenticate to a host, optionally forcing one specific key.
///
/// `-T` asks for no shell (hosts like GitHub refuse one anyway and answer with
/// a greeting), `BatchMode` stops it prompting for a passphrase we cannot
/// answer, and `accept-new` avoids a hang on the unknown-host question while
/// still refusing a host whose key has *changed*.
fn probe_host(host: &str, identity: Option<&Path>) -> (bool, String) {
  let mut cmd = Command::new("ssh");
  cmd
    .arg("-T")
    .arg("-o")
    .arg("BatchMode=yes")
    .arg("-o")
    .arg("StrictHostKeyChecking=accept-new")
    .arg("-o")
    .arg("ConnectTimeout=10");

  if let Some(key) = identity {
    // IdentitiesOnly stops ssh falling back to the agent or another key, which
    // would make the answer about some other key entirely.
    cmd.arg("-o").arg("IdentitiesOnly=yes").arg("-i").arg(key);
  }
  cmd.arg(format!("git@{host}"));

  #[cfg(windows)]
  {
    use std::os::windows::process::CommandExt;
    cmd.creation_flags(crate::git::shell::CREATE_NO_WINDOW);
  }

  let Ok(out) = cmd.output() else {
    return (false, "Could not run ssh on this computer.".to_owned());
  };

  let text = format!(
    "{}{}",
    String::from_utf8_lossy(&out.stdout),
    String::from_utf8_lossy(&out.stderr)
  );
  (is_success(&text), text)
}

/// Whether a host's reply means "I know who you are".
///
/// Exit codes are useless here: GitHub refuses the shell and exits 1 on a
/// perfectly good authentication, so the greeting text is the real signal.
fn is_success(output: &str) -> bool {
  let lower = output.to_lowercase();
  if lower.contains("permission denied") || lower.contains("authentication failed") {
    return false;
  }
  lower.contains("successfully authenticated")
    || lower.contains("does not provide shell access")
    || lower.contains("welcome to gitlab")
    || lower.contains("logged in as")
}

/// Pull the account name out of a host's greeting.
///
/// GitHub says "Hi <name>!", GitLab "Welcome to GitLab, @<name>!". Best-effort:
/// a host we do not recognise simply reports no name rather than a wrong one.
fn parse_identity(output: &str) -> Option<String> {
  for line in output.lines() {
    let line = line.trim();
    if let Some(rest) = line.strip_prefix("Hi ") {
      let name = rest.split(['!', ',']).next()?.trim();
      if !name.is_empty() {
        return Some(name.to_owned());
      }
    }
    if let Some(idx) = line.find("Welcome to GitLab, @") {
      let rest = &line[idx + "Welcome to GitLab, @".len()..];
      let name = rest.split(['!', ',']).next()?.trim();
      if !name.is_empty() {
        return Some(name.to_owned());
      }
    }
  }
  None
}

/// Test whether SSH to `host` works, and if it does not, find a key that would.
pub fn test_host(host: &str) -> SshTestResult {
  let host = host.trim();
  if host.is_empty() {
    return SshTestResult {
      ok: false,
      message: "No host to test.".into(),
      identity: None,
      working_key: None,
    };
  }

  // First as git itself would connect, honouring ~/.ssh/config.
  let (ok, output) = probe_host(host, None);
  if ok {
    let identity = parse_identity(&output);
    let message = match &identity {
      Some(name) => format!("Connected to {host} as {name}."),
      None => format!("Connected to {host}."),
    };
    return SshTestResult {
      ok: true,
      message,
      identity,
      working_key: None,
    };
  }

  // It failed. Try each key on its own to find one the host does accept - the
  // difference between "set up a key" and "you already have the right key, it
  // just is not the one being offered".
  let working = list_keys().into_iter().find_map(|key| {
    let private = PathBuf::from(key.path.trim_end_matches(".pub"));
    if !private.is_file() {
      return None;
    }
    let (ok, out) = probe_host(host, Some(&private));
    ok.then(|| (key, parse_identity(&out)))
  });

  match working {
    Some((key, identity)) => SshTestResult {
      ok: false,
      message: format!(
        "{host} refused the key it was offered, but your {} key works. Switch to it below.",
        key.name
      ),
      identity,
      working_key: Some(key.path),
    },
    None => SshTestResult {
      ok: false,
      message: format!(
        "{host} did not accept any key on this computer. Make a key and add it to your account."
      ),
      identity: None,
      working_key: None,
    },
  }
}

/* ---------------------------------------------------------- ssh config */

/// Which key `~/.ssh/config` currently sends to a host, if it names one.
///
/// Only reads what is written: this does not re-implement ssh's matching rules
/// beyond exact and `*` host patterns, which covers what the app writes and the
/// overwhelmingly common hand-written case.
pub fn configured_key_for(host: &str) -> Option<String> {
  let path = ssh_config_path()?;
  let text = std::fs::read_to_string(path).ok()?;
  parse_configured_key(&text, host)
}

/// Read the effective IdentityFile for a host out of config text.
fn parse_configured_key(text: &str, host: &str) -> Option<String> {
  let mut current: Option<String> = None;
  let mut exact: Option<String> = None;
  let mut wildcard: Option<String> = None;

  for line in text.lines() {
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.starts_with('#') {
      continue;
    }
    let lower = trimmed.to_lowercase();

    if let Some(rest) = lower.strip_prefix("host ") {
      current = Some(rest.trim().to_owned());
      continue;
    }

    if lower.starts_with("identityfile ") {
      // Slice the value off the ORIGINAL line: the lowercased copy would
      // destroy the case of the path.
      let value = trimmed["identityfile ".len()..].trim().to_owned();
      match current.as_deref() {
        Some(patterns) if patterns.split_whitespace().any(|p| p == host) => {
          exact.get_or_insert(value);
        }
        Some(patterns) if patterns.split_whitespace().any(|p| p == "*") => {
          wildcard.get_or_insert(value);
        }
        _ => {}
      }
    }
  }

  // An exact Host entry beats the catch-all, which is how ssh resolves it too.
  exact.or(wildcard)
}

fn ssh_config_path() -> Option<PathBuf> {
  Some(ssh_dir()?.join("config"))
}

/// Point a host at a specific key in `~/.ssh/config`.
///
/// Rewrites only the `IdentityFile` of an existing exact `Host` block, or adds
/// a new block when there is none. Everything else in the file is preserved
/// line for line - people keep real work in here (ports, proxies, per-host
/// users), and a rewrite that "tidied" it would be a betrayal.
///
/// A timestamped backup is written first, because this is the one place the app
/// edits a file it does not own.
pub fn set_key_for_host(host: &str, key_path: &str, stamp: &str) -> Result<(), AppError> {
  let host = host.trim();
  if host.is_empty() {
    return Err(AppError::Other("No host to set a key for.".into()));
  }

  let path =
    ssh_config_path().ok_or_else(|| AppError::Other("Could not find your SSH folder.".into()))?;
  if let Some(parent) = path.parent() {
    std::fs::create_dir_all(parent)?;
  }

  let original = std::fs::read_to_string(&path).unwrap_or_default();
  if !original.is_empty() {
    let backup = path.with_file_name(format!("config.backup-{stamp}"));
    std::fs::write(&backup, &original)?;
  }

  // ssh wants forward slashes; a Windows path with backslashes is read as
  // escapes and silently fails to match any file.
  let key = key_path.replace('\\', "/");
  std::fs::write(&path, rewrite_config(&original, host, &key))?;
  Ok(())
}

/// Produce the new config text. Split out from the file I/O so the rewriting
/// rules can be tested against real config shapes.
pub fn rewrite_config(original: &str, host: &str, key: &str) -> String {
  let mut out: Vec<String> = Vec::new();
  let mut in_target = false;
  let mut replaced = false;
  let mut found_block = false;

  for line in original.lines() {
    let trimmed = line.trim();
    let lower = trimmed.to_lowercase();

    if let Some(rest) = lower.strip_prefix("host ") {
      // Leaving the target block without having seen an IdentityFile: add one
      // before moving on, or the block would keep inheriting the wildcard.
      if in_target && !replaced {
        out.push(format!("  IdentityFile {key}"));
        replaced = true;
      }
      in_target = rest.trim().split_whitespace().any(|p| p == host);
      if in_target {
        found_block = true;
      }
      out.push(line.to_owned());
      continue;
    }

    if in_target && lower.starts_with("identityfile ") {
      if !replaced {
        // Keep the original indentation so the file still looks hand-written.
        let indent: String = line.chars().take_while(|c| c.is_whitespace()).collect();
        out.push(format!("{indent}IdentityFile {key}"));
        replaced = true;
      }
      // Any further IdentityFile lines in this block are dropped: ssh offers
      // them as alternatives, which is what causes the wrong-key confusion.
      continue;
    }

    out.push(line.to_owned());
  }

  if in_target && !replaced {
    out.push(format!("  IdentityFile {key}"));
  }

  if !found_block {
    if !out.is_empty() && !out.last().is_some_and(|l| l.trim().is_empty()) {
      out.push(String::new());
    }
    out.push(format!("Host {host}"));
    out.push(format!("  HostName {host}"));
    out.push("  User git".to_owned());
    out.push(format!("  IdentityFile {key}"));
  }

  let mut text = out.join("\n");
  if !text.ends_with('\n') {
    text.push('\n');
  }
  text
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

  /// A config in the shape people actually keep: a per-host block with a
  /// non-default port, a catch-all, and comments.
  const REAL_CONFIG: &str = "\
Host homeassistant.local
  HostName homeassistant.local
  User root
  IdentityFile ~/.ssh/homeassistant

# GitHub over 443, because 22 is blocked here
Host github.com
  Hostname ssh.github.com
  Port 443
  User git

Host *
    User root
    IdentityFile ~/.ssh/id_ed25519
    ServerAliveInterval 60
";

  #[test]
  fn an_exact_host_entry_beats_the_wildcard() {
    // ssh resolves it this way, and getting it backwards is what makes the
    // "which key is actually used" answer wrong.
    let key = parse_configured_key(REAL_CONFIG, "homeassistant.local");
    assert_eq!(key.as_deref(), Some("~/.ssh/homeassistant"));
  }

  #[test]
  fn a_host_with_no_key_of_its_own_falls_back_to_the_wildcard() {
    // This is the case that caused real confusion: github.com has a block, but
    // that block names no key, so the Host * key is what gets offered.
    let key = parse_configured_key(REAL_CONFIG, "github.com");
    assert_eq!(key.as_deref(), Some("~/.ssh/id_ed25519"));
  }

  #[test]
  fn setting_a_key_on_an_existing_block_keeps_everything_else() {
    // The Port 443 and Hostname lines are load-bearing; losing them would break
    // the connection this is meant to fix.
    let out = rewrite_config(REAL_CONFIG, "github.com", "C:/Users/me/.ssh/work");
    assert!(out.contains("Hostname ssh.github.com"));
    assert!(out.contains("Port 443"));
    assert!(out.contains("IdentityFile C:/Users/me/.ssh/work"));
    // Other blocks are untouched.
    assert!(out.contains("IdentityFile ~/.ssh/homeassistant"));
    assert!(out.contains("ServerAliveInterval 60"));
    // And the comment survives.
    assert!(out.contains("# GitHub over 443"));
  }

  #[test]
  fn setting_a_key_replaces_rather_than_appends() {
    let out = rewrite_config(REAL_CONFIG, "homeassistant.local", "C:/keys/new");
    assert_eq!(out.matches("IdentityFile C:/keys/new").count(), 1);
    assert!(!out.contains("IdentityFile ~/.ssh/homeassistant"));
    // The wildcard's key is a different block and must survive.
    assert!(out.contains("IdentityFile ~/.ssh/id_ed25519"));
  }

  #[test]
  fn a_host_with_no_block_gets_a_new_one() {
    let out = rewrite_config(REAL_CONFIG, "gitlab.com", "C:/keys/lab");
    assert!(out.contains("Host gitlab.com"));
    assert!(out.contains("IdentityFile C:/keys/lab"));
    // Appended, not inserted into someone else's block.
    assert!(out.trim_end().ends_with("IdentityFile C:/keys/lab"));
  }

  #[test]
  fn an_empty_config_gets_a_complete_block() {
    let out = rewrite_config("", "github.com", "C:/keys/k");
    assert!(out.contains("Host github.com"));
    assert!(out.contains("User git"));
    assert!(out.contains("IdentityFile C:/keys/k"));
  }

  #[test]
  fn the_wildcard_block_is_never_mistaken_for_a_host_block() {
    // Writing into Host * would change the key for every host on the machine.
    let out = rewrite_config(REAL_CONFIG, "github.com", "C:/keys/work");
    let wildcard = out.split("Host *").nth(1).unwrap();
    assert!(wildcard.contains("IdentityFile ~/.ssh/id_ed25519"));
    assert!(!wildcard.contains("C:/keys/work"));
  }

  #[test]
  fn github_style_success_is_recognised() {
    // GitHub refuses the shell and exits non-zero on a GOOD authentication, so
    // the text is the only signal.
    assert!(is_success(
      "Hi Wutname1! You've successfully authenticated, but GitHub does not provide shell access."
    ));
  }

  #[test]
  fn permission_denied_is_never_read_as_success() {
    assert!(!is_success("git@ssh.github.com: Permission denied (publickey)."));
    // Even when a success phrase appears alongside it.
    assert!(!is_success(
      "Permission denied (publickey). does not provide shell access"
    ));
  }

  #[test]
  fn the_account_name_is_pulled_from_a_greeting() {
    assert_eq!(
      parse_identity("Hi Wutname1! You've successfully authenticated").as_deref(),
      Some("Wutname1")
    );
    assert_eq!(
      parse_identity("Welcome to GitLab, @jane!").as_deref(),
      Some("jane")
    );
    assert_eq!(parse_identity("some unrelated output"), None);
  }

}
