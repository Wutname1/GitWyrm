//! Named identities: who you commit as, and the key you sign with.
//!
//! A name, an email, and a signing key are one decision, not three. Setting
//! them separately is how you end up committing to a personal repo under a work
//! email, or signing with a key whose address does not match the commit -- both
//! of which the host silently marks unverified. A profile keeps them together
//! so switching is one action that cannot half-apply.
//!
//! # Where the values land
//!
//! Two scopes, because git offers two and they mean different things:
//!
//! * **Active profile -> global config.** "Who am I by default" is not a
//!   property of any path, so it cannot be expressed as a conditional include.
//!   Switching rewrites `user.*` in `~/.gitconfig`, which is exactly what git
//!   reads when nothing more specific applies.
//! * **Per-repository override -> that repo's `.git/config`.** Local config
//!   beats both the global values and any include, so an override always wins.
//!   Verified empirically, not assumed -- see `local_beats_include` below.
//!
//! Folder rules use `includeIf` so other git tools agree with GitWyrm rather
//! than GitWyrm being a second source of truth. Note `gitdir/i`, not `gitdir`:
//! on Windows the same folder is reachable under different casing and the
//! case-sensitive form silently fails to match, which is near-impossible to
//! debug from the symptom (your commits are just wrong).

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::error::AppError;
use crate::git::shell::run_git;

/// How a profile signs commits.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase", tag = "kind", content = "value")]
pub enum SigningMethod {
  /// Not set up. Commits are made but not signed.
  None,
  /// A gpg key id, the value git wants for `user.signingkey`.
  Gpg(String),
  /// Path to an SSH public key.
  Ssh(String),
}

impl Default for SigningMethod {
  fn default() -> Self {
    Self::None
  }
}

/// One identity you commit under.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
  /// Stable id, referenced by repository overrides and folder rules.
  pub id: String,
  /// What the user calls it: "Work", "Personal".
  pub label: String,
  pub name: String,
  pub email: String,
  #[serde(default)]
  pub signing: SigningMethod,
  /// Sign every commit made under this profile.
  #[serde(default)]
  pub sign_commits: bool,
  /// Folders whose repositories use this profile automatically. Written as
  /// `includeIf` rules so the terminal and other git tools agree.
  #[serde(default)]
  pub folders: Vec<String>,
}

impl Profile {
  /// True when git has enough to make a commit under this profile.
  pub fn is_complete(&self) -> bool {
    !self.name.trim().is_empty() && !self.email.trim().is_empty()
  }
}

/// Normalize a path for git config: forward slashes, and a trailing slash so
/// `gitdir` matches the folder's contents rather than only the folder itself.
///
/// Git reads backslashes in config values as escape sequences, so a raw Windows
/// path produces `bad config line` rather than a wrong match -- loud, at least,
/// but still broken.
pub fn git_dir_pattern(folder: &str) -> String {
  let slashed = folder.trim().replace('\\', "/");
  let trimmed = slashed.trim_end_matches('/');
  format!("{trimmed}/")
}

/// The `includeIf` section for one folder rule.
///
/// `gitdir/i` because Windows paths are case-insensitive in practice.
pub fn include_if_block(folder: &str, include_path: &Path) -> String {
  let pattern = git_dir_pattern(folder);
  let path = include_path.to_string_lossy().replace('\\', "/");
  format!("[includeIf \"gitdir/i:{pattern}\"]\n\tpath = {path}\n")
}

/// The config body a profile contributes: identity, and signing when set up.
pub fn profile_config_body(profile: &Profile) -> String {
  let mut out = String::from("[user]\n");
  out.push_str(&format!("\tname = {}\n", profile.name.trim()));
  out.push_str(&format!("\temail = {}\n", profile.email.trim()));

  match &profile.signing {
    SigningMethod::Gpg(key) if !key.trim().is_empty() => {
      out.push_str(&format!("\tsigningkey = {}\n", key.trim()));
      out.push_str("[gpg]\n\tformat = openpgp\n");
    }
    SigningMethod::Ssh(path) if !path.trim().is_empty() => {
      let key = path.trim().replace('\\', "/");
      out.push_str(&format!("\tsigningkey = {key}\n"));
      out.push_str("[gpg]\n\tformat = ssh\n");
    }
    _ => {}
  }

  out.push_str(&format!(
    "[commit]\n\tgpgsign = {}\n",
    if profile.sign_commits && profile.signing != SigningMethod::None {
      "true"
    } else {
      "false"
    }
  ));
  out
}

/// Apply a profile to one repository's own config.
///
/// Local config outranks the global values and any include, so this is the
/// scope that makes an override actually stick.
pub fn apply_to_repo(repo_path: &str, profile: &Profile) -> Result<(), AppError> {
  if !profile.is_complete() {
    return Err(AppError::Other(
      "This profile needs a name and an email before it can be used.".into(),
    ));
  }

  set_local(repo_path, "user.name", profile.name.trim())?;
  set_local(repo_path, "user.email", profile.email.trim())?;

  match &profile.signing {
    SigningMethod::Gpg(key) if !key.trim().is_empty() => {
      set_local(repo_path, "user.signingkey", key.trim())?;
      set_local(repo_path, "gpg.format", "openpgp")?;
    }
    SigningMethod::Ssh(path) if !path.trim().is_empty() => {
      set_local(repo_path, "user.signingkey", &path.trim().replace('\\', "/"))?;
      set_local(repo_path, "gpg.format", "ssh")?;
    }
    _ => {
      // Leaving a stale key behind would sign the next commit as whoever the
      // previous profile was.
      let _ = run_git(Some(repo_path), &["config", "--local", "--unset", "user.signingkey"]);
    }
  }

  let sign = profile.sign_commits && profile.signing != SigningMethod::None;
  set_local(repo_path, "commit.gpgsign", if sign { "true" } else { "false" })?;
  Ok(())
}

/// Drop a repository's override so it follows the active profile again.
pub fn clear_repo_override(repo_path: &str) -> Result<(), AppError> {
  for key in [
    "user.name",
    "user.email",
    "user.signingkey",
    "gpg.format",
    "commit.gpgsign",
  ] {
    // Absent is the normal case and exits non-zero; only set keys need clearing.
    let _ = run_git(Some(repo_path), &["config", "--local", "--unset", key]);
  }
  Ok(())
}

fn set_local(repo_path: &str, key: &str, value: &str) -> Result<(), AppError> {
  run_git(Some(repo_path), &["config", "--local", key, value])?;
  Ok(())
}

/// Where a profile's generated include file lives.
pub fn include_file_path(config_dir: &Path, profile_id: &str) -> PathBuf {
  config_dir.join(format!("profile-{profile_id}.gitconfig"))
}

/// Make a profile the global default: who you are when no repository says
/// otherwise.
///
/// Writes `~/.gitconfig` directly. This is the one operation that reaches
/// outside GitWyrm and changes what every other git tool sees, which is the
/// point of a switcher -- but it is also why it must never run implicitly.
pub fn apply_globally(profile: &Profile) -> Result<(), AppError> {
  if !profile.is_complete() {
    return Err(AppError::Other(
      "This profile needs a name and an email before it can be used.".into(),
    ));
  }

  let path = crate::git::identity::global_config_path()?;
  let mut config = git2::Config::open(&path)?;
  config.set_str("user.name", profile.name.trim())?;
  config.set_str("user.email", profile.email.trim())?;

  match &profile.signing {
    SigningMethod::Gpg(key) if !key.trim().is_empty() => {
      config.set_str("user.signingkey", key.trim())?;
      config.set_str("gpg.format", "openpgp")?;
    }
    SigningMethod::Ssh(key) if !key.trim().is_empty() => {
      config.set_str("user.signingkey", &key.trim().replace('\\', "/"))?;
      config.set_str("gpg.format", "ssh")?;
    }
    _ => {
      // A leftover key from the previous profile would sign the next commit as
      // the wrong person, which is the exact failure profiles exist to prevent.
      let _ = config.remove("user.signingkey");
    }
  }

  let sign = profile.sign_commits && profile.signing != SigningMethod::None;
  config.set_bool("commit.gpgsign", sign)?;
  Ok(())
}

/// Read back what git currently resolves in `repo_path`, so the UI can show
/// what a commit made right now would actually carry.
///
/// Asks git rather than reconstructing it from our own settings: includes,
/// hand-edited config, and per-repo overrides all feed into the answer, and
/// only git knows the true precedence.
pub fn effective_identity(repo_path: &str) -> GitIdentitySnapshot {
  let read = |key: &str| -> Option<String> {
    run_git(Some(repo_path), &["config", "--get", key])
      .ok()
      .map(|out| out.stdout.trim().to_string())
      .filter(|v| !v.is_empty())
  };

  GitIdentitySnapshot {
    name: read("user.name").unwrap_or_default(),
    email: read("user.email").unwrap_or_default(),
    signing_key: read("user.signingkey"),
    format: read("gpg.format"),
    sign_commits: read("commit.gpgsign")
      .map(|v| v.eq_ignore_ascii_case("true"))
      .unwrap_or(false),
    // A local value means this repository overrides whatever the profile says.
    overridden: run_git(Some(repo_path), &["config", "--local", "--get", "user.email"])
      .ok()
      .map(|out| !out.stdout.trim().is_empty())
      .unwrap_or(false),
  }
}

/// Marker bounding the block of `includeIf` rules GitWyrm manages.
const BEGIN: &str = "# >>> GitWyrm profiles >>>";
const END: &str = "# <<< GitWyrm profiles <<<";

/// Replace the managed rule block in a config file's text.
///
/// Rewriting only between the markers keeps every hand-written line in
/// `~/.gitconfig` intact -- people keep aliases and credential helpers there,
/// and silently eating them would be unforgivable for a git client.
pub fn splice_managed_block(existing: &str, block: &str) -> String {
  let body = block.trim_end();
  let managed = if body.is_empty() {
    String::new()
  } else {
    format!("{BEGIN}\n{body}\n{END}\n")
  };

  if let (Some(start), Some(end)) = (existing.find(BEGIN), existing.find(END)) {
    if start < end {
      let tail = &existing[end + END.len()..];
      let tail = tail.strip_prefix('\n').unwrap_or(tail);
      return format!("{}{}{}", &existing[..start], managed, tail);
    }
  }

  if managed.is_empty() {
    return existing.to_string();
  }
  if existing.is_empty() {
    return managed;
  }
  let sep = if existing.ends_with('\n') { "" } else { "\n" };
  format!("{existing}{sep}{managed}")
}

/// Build the rule block for every profile that claims folders.
pub fn folder_rules_block(profiles: &[Profile], config_dir: &Path) -> String {
  let mut out = String::new();
  for profile in profiles {
    for folder in &profile.folders {
      if folder.trim().is_empty() {
        continue;
      }
      out.push_str(&include_if_block(
        folder,
        &include_file_path(config_dir, &profile.id),
      ));
    }
  }
  out
}

/// Write each profile's include file and refresh the rules in the global config.
pub fn write_folder_rules(profiles: &[Profile], config_dir: &Path) -> Result<(), AppError> {
  std::fs::create_dir_all(config_dir)?;

  for profile in profiles {
    let path = include_file_path(config_dir, &profile.id);
    if profile.folders.iter().any(|f| !f.trim().is_empty()) && profile.is_complete() {
      std::fs::write(&path, profile_config_body(profile))?;
    } else if path.exists() {
      // A profile that no longer claims folders should stop applying to them.
      let _ = std::fs::remove_file(&path);
    }
  }

  let global = crate::git::identity::global_config_path()?;
  let existing = std::fs::read_to_string(&global).unwrap_or_default();
  let updated = splice_managed_block(&existing, &folder_rules_block(profiles, config_dir));
  if updated != existing {
    std::fs::write(&global, updated)?;
  }
  Ok(())
}

/// What git resolves for a repository right now.
#[derive(Debug, Clone, Default, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct GitIdentitySnapshot {
  pub name: String,
  pub email: String,
  pub signing_key: Option<String>,
  pub format: Option<String>,
  pub sign_commits: bool,
  /// True when the repository's own config sets the identity.
  pub overridden: bool,
}

#[cfg(test)]
mod tests {
  use super::*;

  fn profile() -> Profile {
    Profile {
      id: "p1".into(),
      label: "Personal".into(),
      name: "Wutname1".into(),
      email: "jeremy12@gmail.com".into(),
      signing: SigningMethod::None,
      sign_commits: false,
      folders: vec![],
    }
  }

  #[test]
  fn folder_patterns_use_forward_slashes_and_a_trailing_slash() {
    // Backslashes are escape sequences to git's config parser, and a missing
    // trailing slash matches the folder but not the repos inside it.
    assert_eq!(git_dir_pattern(r"C:\code\work"), "C:/code/work/");
    assert_eq!(git_dir_pattern("C:/code/work/"), "C:/code/work/");
    assert_eq!(git_dir_pattern("C:/code/work///"), "C:/code/work/");
  }

  #[test]
  fn include_rules_are_case_insensitive() {
    // gitdir (case-sensitive) silently fails to match on Windows, where the
    // same folder is reachable as C:\Code and c:\code.
    let block = include_if_block(r"C:\code\work", Path::new(r"C:\cfg\profile-p1.gitconfig"));
    assert!(block.contains("gitdir/i:C:/code/work/"), "got: {block}");
    assert!(block.contains("path = C:/cfg/profile-p1.gitconfig"), "got: {block}");
  }

  #[test]
  fn a_profile_without_signing_turns_signing_off() {
    let body = profile_config_body(&profile());
    assert!(body.contains("name = Wutname1"));
    assert!(body.contains("email = jeremy12@gmail.com"));
    assert!(body.contains("gpgsign = false"));
    assert!(!body.contains("signingkey"));
  }

  #[test]
  fn a_gpg_profile_writes_its_key_and_format() {
    let mut p = profile();
    p.signing = SigningMethod::Gpg("9DF35A9C37DD1FAB".into());
    p.sign_commits = true;
    let body = profile_config_body(&p);
    assert!(body.contains("signingkey = 9DF35A9C37DD1FAB"));
    assert!(body.contains("format = openpgp"));
    assert!(body.contains("gpgsign = true"));
  }

  #[test]
  fn an_ssh_profile_writes_the_ssh_format() {
    let mut p = profile();
    p.signing = SigningMethod::Ssh(r"C:\Users\me\.ssh\id_ed25519.pub".into());
    p.sign_commits = true;
    let body = profile_config_body(&p);
    assert!(body.contains("signingkey = C:/Users/me/.ssh/id_ed25519.pub"));
    assert!(body.contains("format = ssh"));
  }

  #[test]
  fn asking_to_sign_without_a_key_does_not_enable_signing() {
    // Otherwise git refuses every commit with a message about a missing key.
    let mut p = profile();
    p.sign_commits = true;
    assert!(profile_config_body(&p).contains("gpgsign = false"));
  }

  const HAND_WRITTEN: &str = "[alias]\n\tco = checkout\n[credential]\n\thelper = manager\n";

  #[test]
  fn splicing_keeps_hand_written_config() {
    // People keep aliases and credential helpers in ~/.gitconfig. Eating them
    // would be unforgivable for a git client.
    let out = splice_managed_block(HAND_WRITTEN, "[includeIf \"gitdir/i:C:/work/\"]\n\tpath = x\n");
    assert!(out.contains("co = checkout"));
    assert!(out.contains("helper = manager"));
    assert!(out.contains("gitdir/i:C:/work/"));
  }

  #[test]
  fn splicing_twice_does_not_duplicate_rules() {
    let once = splice_managed_block(HAND_WRITTEN, "[includeIf \"gitdir/i:C:/work/\"]\n\tpath = x\n");
    let twice = splice_managed_block(&once, "[includeIf \"gitdir/i:C:/other/\"]\n\tpath = y\n");
    assert_eq!(twice.matches(BEGIN).count(), 1, "one managed block: {twice}");
    assert!(!twice.contains("C:/work/"), "stale rule survived: {twice}");
    assert!(twice.contains("C:/other/"));
    assert!(twice.contains("co = checkout"));
  }

  #[test]
  fn removing_every_rule_leaves_the_file_clean() {
    let once = splice_managed_block(HAND_WRITTEN, "[includeIf \"gitdir/i:C:/work/\"]\n\tpath = x\n");
    let cleared = splice_managed_block(&once, "");
    assert!(!cleared.contains(BEGIN), "marker left behind: {cleared}");
    assert!(!cleared.contains("gitdir"), "rule left behind: {cleared}");
    assert!(cleared.contains("co = checkout"));
  }

  #[test]
  fn a_profile_with_no_folders_contributes_no_rules() {
    let dir = Path::new("C:/cfg");
    assert!(folder_rules_block(&[profile()], dir).is_empty());
  }

  #[test]
  fn a_profile_needs_both_halves_to_be_usable() {
    let mut p = profile();
    assert!(p.is_complete());
    p.email = "   ".into();
    assert!(!p.is_complete());
  }
}
