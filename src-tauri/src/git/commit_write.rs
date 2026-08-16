//! Creating commit objects in a way that honours the user's signing config.
//!
//! libgit2 does not read `commit.gpgsign`. Anything built with `repo.commit()`
//! or `Commit::amend()` is unsigned no matter what the config says -- so a user
//! who set up a signing profile in Settings got quietly unsigned commits, while
//! the same repository committed from the command line got signed ones.
//!
//! Signing is delegated to `git commit-tree -S` rather than produced here. Git
//! dispatches on `gpg.format` (openpgp/ssh/x509), honours `gpg.program` and
//! `gpg.ssh.program`, resolves `user.signingkey`, and drives pinentry; all of
//! that is `gpg-interface.c`, and reimplementing it on top of
//! `Repository::commit_signed()` would be a second copy to keep in step with
//! git. `commit-tree` is plumbing: handed a tree and parents it returns an oid
//! and touches nothing else -- not the index, the working tree, or HEAD -- so
//! it drops into flows that build their own trees (amend, the AI commit chain)
//! where a porcelain `git commit` could not express the tree at all.

use std::process::{Command, Stdio};

use std::io::Write;

use crate::error::AppError;
use crate::git::shell::git_program_name;

/// A repository's working directory, as the string the git CLI wants.
///
/// Saves threading a path through call sites that already hold the repository
/// and nothing else. Bare repositories have no workdir; they are rejected at
/// open time, so falling back to the git directory only matters in tests.
pub fn workdir_of(repo: &git2::Repository) -> String {
  repo
    .workdir()
    .unwrap_or_else(|| repo.path())
    .to_string_lossy()
    .into_owned()
}

/// Whether git would sign a commit made in this repository right now.
///
/// Asks git rather than reading our own settings: the value can come from
/// local config, the global file, or an `includeIf` rule, and only git knows
/// the precedence. `--type=bool` normalises the spellings git accepts ("yes",
/// "on", "1") into a plain `true`/`false`, so callers do not have to.
///
/// A missing key exits non-zero, which is the ordinary "not configured" case
/// and reads as "do not sign".
pub fn signing_enabled(repo_path: &str) -> bool {
  crate::git::shell::run_git(
    Some(repo_path),
    &["config", "--type=bool", "--get", "commit.gpgsign"],
  )
  .map(|out| out.stdout.trim() == "true")
  .unwrap_or(false)
}

/// Author and committer for a commit, in the form `git commit-tree` wants.
///
/// Carried separately from the message because amend keeps the original
/// author while taking the current user as committer.
pub struct CommitIdentity {
  pub author: git2::Signature<'static>,
  pub committer: git2::Signature<'static>,
}

/// Build a signed commit object with `git commit-tree -S` and return its oid.
///
/// Does not move any reference -- the caller updates HEAD, exactly as it would
/// after `repo.commit(None, ..)`. Author and committer are passed through the
/// environment so the commit records the same identity libgit2 would have
/// written, including the timestamp and offset.
fn commit_tree_signed(
  repo_path: &str,
  tree: git2::Oid,
  parents: &[git2::Oid],
  message: &str,
  identity: &CommitIdentity,
) -> Result<git2::Oid, AppError> {
  let tree = tree.to_string();
  let mut args: Vec<String> = vec![
    "-C".into(),
    repo_path.into(),
    "commit-tree".into(),
    "-S".into(),
    tree,
  ];
  for parent in parents {
    args.push("-p".into());
    args.push(parent.to_string());
  }

  let mut cmd = Command::new(git_program_name());
  cmd
    .args(&args)
    .stdin(Stdio::piped())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped());

  apply_identity_env(&mut cmd, identity);

  #[cfg(windows)]
  {
    use std::os::windows::process::CommandExt;
    cmd.creation_flags(crate::git::shell::CREATE_NO_WINDOW);
  }

  let mut child = cmd.spawn().map_err(|e| {
    if e.kind() == std::io::ErrorKind::NotFound {
      AppError::Other("git executable not found on PATH".into())
    } else {
      AppError::Io(e)
    }
  })?;

  // The message goes in on stdin so it is never parsed as an argument: a
  // message beginning with a dash, or carrying a newline, is otherwise
  // mangled or rejected.
  child
    .stdin
    .take()
    .ok_or_else(|| AppError::Other("failed to open git stdin".into()))?
    .write_all(message.as_bytes())
    .map_err(AppError::Io)?;

  let out = child.wait_with_output().map_err(AppError::Io)?;
  if !out.status.success() {
    let stderr = String::from_utf8_lossy(&out.stderr);
    return Err(AppError::Other(signing_failure_message(stderr.trim())));
  }

  let oid = String::from_utf8_lossy(&out.stdout).trim().to_owned();
  Ok(git2::Oid::from_str(&oid)?)
}

/// Hand git the author and committer so the signed commit carries the same
/// identity and timestamp the unsigned path would have recorded.
///
/// Git reads these from the environment when they are set, which avoids
/// depending on whatever `user.name`/`user.email` resolves to in the child
/// process -- the two can differ, and a commit that changed identity purely
/// because it was signed would be a surprise.
fn apply_identity_env(cmd: &mut Command, identity: &CommitIdentity) {
  let mut set = |who: &str, sig: &git2::Signature<'_>| {
    cmd.env(format!("GIT_{who}_NAME"), sig.name().unwrap_or_default());
    cmd.env(format!("GIT_{who}_EMAIL"), sig.email().unwrap_or_default());
    cmd.env(format!("GIT_{who}_DATE"), rfc2822_ish(sig));
  };
  set("AUTHOR", &identity.author);
  set("COMMITTER", &identity.committer);
}

/// A signature's time in git's own `<unix-seconds> <+/-HHMM>` format.
///
/// Git accepts this "raw" form for `GIT_AUTHOR_DATE` and it round-trips the
/// offset exactly, which the human-readable forms do not always do.
fn rfc2822_ish(sig: &git2::Signature<'_>) -> String {
  let time = sig.when();
  let offset = time.offset_minutes();
  let sign = if offset < 0 { '-' } else { '+' };
  let abs = offset.abs();
  format!("{} {}{:02}{:02}", time.seconds(), sign, abs / 60, abs % 60)
}

/// Turn git's signing diagnostics into something a beginner can act on.
///
/// The raw errors name internals ("gpg failed to sign the data") and say
/// nothing about what to do, which for the people this app is for is a dead
/// end. The original text is kept on the end so a real diagnosis is still
/// possible from the log.
fn signing_failure_message(stderr: &str) -> String {
  let lower = stderr.to_lowercase();

  let hint = if lower.contains("secret key not available")
    || lower.contains("no secret key")
    // gpg reports a key it cannot find as a skipped signer rather than a
    // missing secret key. The status line is the reliable marker: `INV_SGNR`
    // means "invalid signer", whatever errno it decided to pair with it.
    || lower.contains("inv_sgnr")
    || lower.contains("skipped")
  {
    "The signing key this repository uses is missing. Pick a different key in Settings > Security, or turn signing off."
  } else if lower.contains("keyboxd") || lower.contains("no keybox daemon") {
    // gpg could not reach its own key database. Almost always a relocated gpg
    // resolving POSIX paths that do not exist on Windows, which reads to the
    // user as a broken key when the key is fine.
    "GitWyrm could not reach your keyring to sign this commit. Open Settings > Security and check the gpg it is using, or turn signing off."
  } else if lower.contains("no pinentry") || lower.contains("no agent running") {
    "Your signing key needs a passphrase but nothing could ask you for it. Try again, or turn signing off in Settings > Security."
  } else if lower.contains("failed to sign") || lower.contains("gpg") || lower.contains("ssh") {
    "Your commit could not be signed. Check your signing key in Settings > Security, or turn signing off."
  } else {
    "Your commit could not be created."
  };

  // The raw text stays on the error so the log and the copy button keep a real
  // diagnosis, but it goes after a marker the UI splits on. Concatenating it
  // straight onto the hint put a screenful of gpg internals in a toast, which
  // nobody reads and which buried the one sentence that said what to do.
  format!("{hint}{DETAIL_SEPARATOR}{stderr}")
}

/// Separates the sentence a user acts on from the diagnostics behind it.
///
/// Deliberately not a bare blank line: plenty of messages contain those, and
/// splitting on one would truncate them mid-sentence.
pub const DETAIL_SEPARATOR: &str = "\n\u{241E}\n";

/// Point a reference at `oid`, following it first if it is symbolic.
///
/// "HEAD" is normally a symbolic ref to a branch. Writing it directly with
/// `Repository::reference` succeeds and replaces the symref with a direct one,
/// which silently DETACHES HEAD -- and it does so most visibly on the first
/// commit in a repository, where the branch does not exist yet so there is no
/// resolution failure to fall back from. `repo.commit(Some("HEAD"), ..)`
/// resolves the symref and creates the branch instead, so this has to do the
/// same to keep the signed and unsigned paths equivalent.
fn update_symbolic_aware(
  repo: &git2::Repository,
  name: &str,
  oid: git2::Oid,
  reflog: &str,
) -> Result<(), AppError> {
  // Follow a symbolic ref to the branch it names, whether or not that branch
  // exists yet: an unborn branch is exactly the first-commit case.
  let target = match repo.find_reference(name) {
    Ok(reference) if reference.kind() == Some(git2::ReferenceType::Symbolic) => reference
      .symbolic_target()
      .ok()
      .flatten()
      .map(str::to_owned)
      .unwrap_or_else(|| name.to_owned()),
    // No such reference: HEAD on an unborn branch reports itself this way.
    Err(_) if name == "HEAD" => repo
      .find_reference("HEAD")
      .ok()
      .and_then(|r| r.symbolic_target().ok().flatten().map(str::to_owned))
      .unwrap_or_else(|| name.to_owned()),
    _ => name.to_owned(),
  };

  repo.reference(&target, oid, true, reflog)?;
  Ok(())
}

/// Create a commit, signing it when the repository is configured to sign.
///
/// The unsigned path stays on libgit2: it is faster, needs no child process,
/// and is what every repository without signing configured will take.
///
/// `update_ref` mirrors `Repository::commit`: `Some("HEAD")` moves HEAD to the
/// new commit, `None` leaves every reference alone.
pub fn create(
  repo: &git2::Repository,
  repo_path: &str,
  update_ref: Option<&str>,
  identity: &CommitIdentity,
  message: &str,
  tree: &git2::Tree<'_>,
  parents: &[&git2::Commit<'_>],
) -> Result<git2::Oid, AppError> {
  if !signing_enabled(repo_path) {
    return Ok(repo.commit(
      update_ref,
      &identity.author,
      &identity.committer,
      message,
      tree,
      parents,
    )?);
  }

  let parent_oids: Vec<git2::Oid> = parents.iter().map(|c| c.id()).collect();
  let oid = commit_tree_signed(repo_path, tree.id(), &parent_oids, message, identity)?;

  // commit-tree only writes the object. Moving the reference is left to us so
  // the signed and unsigned paths end in the same state.
  if let Some(name) = update_ref {
    let reflog = format!("commit: {}", message.lines().next().unwrap_or_default());
    update_symbolic_aware(repo, name, oid, &reflog)?;
  }

  Ok(oid)
}

/// Amend HEAD, signing the result when the repository is configured to sign.
///
/// Mirrors `Commit::amend` with the arguments this app actually uses: the
/// original author is kept, the current user becomes committer, and the tree
/// is replaced so staged changes fold in.
pub fn amend_head(
  repo: &git2::Repository,
  repo_path: &str,
  head: &git2::Commit<'_>,
  identity: &CommitIdentity,
  message: &str,
  tree: &git2::Tree<'_>,
) -> Result<git2::Oid, AppError> {
  if !signing_enabled(repo_path) {
    return Ok(head.amend(
      Some("HEAD"),
      Some(&identity.author),
      Some(&identity.committer),
      None,
      Some(message),
      Some(tree),
    )?);
  }

  // An amend is a new commit onto the original's parents, which is what the
  // signed path has to build explicitly. A previous signature is not carried
  // over: it covered a different commit and would no longer verify.
  let parents: Vec<git2::Commit<'_>> = head.parents().collect();
  let parent_refs: Vec<&git2::Commit<'_>> = parents.iter().collect();
  create(repo, repo_path, Some("HEAD"), identity, message, tree, &parent_refs)
}

#[cfg(test)]
mod tests {
  use super::*;

  fn sig(offset_minutes: i32) -> git2::Signature<'static> {
    git2::Signature::new(
      "Test User",
      "test@example.com",
      &git2::Time::new(1_700_000_000, offset_minutes),
      )
    .unwrap()
  }

  /// The real stderr from the report that prompted this: gpg reports a key it
  /// cannot find as a skipped signer, not as a missing secret key, so the
  /// original matcher fell through to the vague catch-all.
  const MISSING_KEY_STDERR: &str = "\
error: gpg failed to sign the data:
gpg: skipped \"32BD8D9B66ABAD8B\": Input/output error
[GNUPG:] INV_SGNR 0 32BD8D9B66ABAD8B
gpg: signing failed: Input/output error";

  #[test]
  fn a_skipped_signer_is_reported_as_a_missing_key() {
    let msg = signing_failure_message(MISSING_KEY_STDERR);
    let hint = msg.split(DETAIL_SEPARATOR).next().unwrap();
    assert!(
      hint.contains("no longer exists") || hint.contains("is missing"),
      "expected a missing-key hint, got: {hint}"
    );
  }

  #[test]
  fn the_hint_is_one_short_sentence_not_a_wall_of_gpg_output() {
    let msg = signing_failure_message(MISSING_KEY_STDERR);
    let hint = msg.split(DETAIL_SEPARATOR).next().unwrap();
    // The whole point: a toast shows this, so gpg internals must not be in it.
    assert!(!hint.contains("gpg:"), "raw gpg output leaked: {hint}");
    assert!(!hint.contains("GNUPG"), "status lines leaked: {hint}");
    assert!(hint.len() < 200, "hint is too long for a toast: {hint}");
  }

  /// The details still have to survive -- the log and the copy button need a
  /// real diagnosis, they just must not be in the headline.
  #[test]
  fn the_raw_diagnostics_are_kept_behind_the_separator() {
    let msg = signing_failure_message(MISSING_KEY_STDERR);
    let (_, detail) = msg.split_once(DETAIL_SEPARATOR).expect("separator present");
    assert!(detail.contains("INV_SGNR"));
    assert!(detail.contains("32BD8D9B66ABAD8B"));
  }

  #[test]
  fn a_broken_keyring_is_named_rather_than_blamed_on_the_key() {
    // gpg could not reach its key database. The key is fine; saying "check your
    // signing key" sends the user to fix something that is not broken.
    let msg = signing_failure_message(
      "gpg: error running '/usr/lib/gnupg/keyboxd': probably not installed\n\
       gpg: can't connect to the keyboxd: Configuration error",
    );
    let hint = msg.split(DETAIL_SEPARATOR).next().unwrap();
    assert!(hint.contains("keyring"), "got: {hint}");
  }

  #[test]
  fn dates_use_gits_raw_seconds_and_offset_form() {
    // The format git parses for GIT_AUTHOR_DATE without reinterpreting the
    // zone, so a commit keeps the offset it was made in.
    assert_eq!(rfc2822_ish(&sig(0)), "1700000000 +0000");
    assert_eq!(rfc2822_ish(&sig(120)), "1700000000 +0200");
  }

  #[test]
  fn negative_offsets_keep_their_sign_and_pad() {
    // -330 is India-style half-hour offset in the other direction; the minutes
    // remainder has to survive, not get rounded into the hour.
    assert_eq!(rfc2822_ish(&sig(-330)), "1700000000 -0530");
    assert_eq!(rfc2822_ish(&sig(-60)), "1700000000 -0100");
  }

  #[test]
  fn a_missing_key_is_explained_as_a_key_problem() {
    let msg = signing_failure_message("error: gpg failed to sign the data\nsecret key not available");
    assert!(msg.contains("Settings > Security"));
    assert!(msg.contains("missing"));
    // The raw text stays for diagnosis.
    assert!(msg.contains("secret key not available"));
  }

  #[test]
  fn a_missing_agent_is_explained_as_a_passphrase_problem() {
    let msg = signing_failure_message("gpg: no pinentry\ngpg: signing failed: No pinentry");
    assert!(msg.contains("passphrase"));
  }

  #[test]
  fn an_unrecognised_failure_still_says_something_useful() {
    let msg = signing_failure_message("fatal: something else went wrong");
    assert!(msg.contains("could not be created"));
    assert!(msg.contains("something else went wrong"));
  }

  /// Build a repository with one commit, returning it and its path.
  fn repo_with_base() -> (tempfile::TempDir, git2::Repository) {
    let dir = tempfile::tempdir().unwrap();
    let repo = git2::Repository::init(dir.path()).unwrap();
    {
      let sig = sig(0);
      let mut index = repo.index().unwrap();
      std::fs::write(dir.path().join("a.txt"), "one").unwrap();
      index.add_path(std::path::Path::new("a.txt")).unwrap();
      index.write().unwrap();
      let tree_oid = index.write_tree().unwrap();
      let tree = repo.find_tree(tree_oid).unwrap();
      repo.commit(Some("HEAD"), &sig, &sig, "base", &tree, &[]).unwrap();
    }
    (dir, repo)
  }

  #[test]
  fn signing_is_off_when_the_config_says_nothing() {
    let (dir, _repo) = repo_with_base();
    assert!(!signing_enabled(&dir.path().to_string_lossy()));
  }

  #[test]
  fn signing_reads_the_repositorys_config() {
    let (dir, repo) = repo_with_base();
    let path = dir.path().to_string_lossy().into_owned();

    repo.config().unwrap().set_bool("commit.gpgsign", true).unwrap();
    assert!(signing_enabled(&path));

    repo.config().unwrap().set_bool("commit.gpgsign", false).unwrap();
    assert!(!signing_enabled(&path));
  }

  #[test]
  fn gits_other_spellings_of_true_are_understood() {
    // git accepts "yes"/"on"/"1"; --type=bool is what normalises them, and a
    // plain string compare against "true" would read these as "do not sign".
    let (dir, repo) = repo_with_base();
    let path = dir.path().to_string_lossy().into_owned();

    for spelling in ["yes", "on", "1", "TRUE"] {
      repo.config().unwrap().set_str("commit.gpgsign", spelling).unwrap();
      assert!(signing_enabled(&path), "{spelling} should mean sign");
    }
  }

  #[test]
  fn an_unsigned_commit_still_moves_head() {
    // The unsigned path must keep behaving exactly as repo.commit did.
    let (dir, repo) = repo_with_base();
    let path = dir.path().to_string_lossy().into_owned();

    let head = repo.head().unwrap().peel_to_commit().unwrap();
    std::fs::write(dir.path().join("b.txt"), "two").unwrap();
    let mut index = repo.index().unwrap();
    index.add_path(std::path::Path::new("b.txt")).unwrap();
    index.write().unwrap();
    let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();

    let identity = CommitIdentity { author: sig(0), committer: sig(0) };
    let oid = create(&repo, &path, Some("HEAD"), &identity, "second", &tree, &[&head]).unwrap();

    assert_eq!(repo.head().unwrap().peel_to_commit().unwrap().id(), oid);
    assert_eq!(repo.find_commit(oid).unwrap().message().unwrap(), "second");
  }

  #[test]
  fn the_first_commit_creates_the_branch_instead_of_detaching_head() {
    // Regression: writing "HEAD" directly replaces the symbolic ref with a
    // direct one, detaching HEAD. It bites hardest on the very first commit,
    // where the branch does not exist yet -- every later history rewrite then
    // fails with "HEAD is detached; check out a branch first".
    let dir = tempfile::tempdir().unwrap();
    let repo = git2::Repository::init(dir.path()).unwrap();
    let path = dir.path().to_string_lossy().into_owned();
    let branch = repo.find_reference("HEAD").unwrap().symbolic_target().unwrap().unwrap().to_owned();

    std::fs::write(dir.path().join("a.txt"), "one").unwrap();
    let mut index = repo.index().unwrap();
    index.add_path(std::path::Path::new("a.txt")).unwrap();
    index.write().unwrap();
    let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();

    let identity = CommitIdentity { author: sig(0), committer: sig(0) };
    let oid = create(&repo, &path, Some("HEAD"), &identity, "first", &tree, &[]).unwrap();

    assert!(!repo.head_detached().unwrap(), "HEAD must stay on a branch");
    assert_eq!(repo.find_reference(&branch).unwrap().target().unwrap(), oid);
    assert!(repo.head().unwrap().is_branch());
  }

  #[test]
  fn later_commits_move_the_branch_not_head_itself() {
    let dir = tempfile::tempdir().unwrap();
    let repo = git2::Repository::init(dir.path()).unwrap();
    let path = dir.path().to_string_lossy().into_owned();
    let branch = repo.find_reference("HEAD").unwrap().symbolic_target().unwrap().unwrap().to_owned();
    let identity = CommitIdentity { author: sig(0), committer: sig(0) };

    for (name, body) in [("a.txt", "one"), ("b.txt", "two")] {
      std::fs::write(dir.path().join(name), body).unwrap();
      let mut index = repo.index().unwrap();
      index.add_path(std::path::Path::new(name)).unwrap();
      index.write().unwrap();
      let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
      let head = repo.head().ok().and_then(|h| h.peel_to_commit().ok());
      let parents: Vec<&git2::Commit> = head.iter().collect();
      create(&repo, &path, Some("HEAD"), &identity, name, &tree, &parents).unwrap();
    }

    assert!(!repo.head_detached().unwrap(), "HEAD must stay on a branch");
    assert_eq!(repo.find_reference(&branch).unwrap().peel_to_commit().unwrap().message().unwrap(), "b.txt");
  }

  #[test]
  fn an_unsigned_amend_replaces_head_and_keeps_the_parent() {
    let (dir, repo) = repo_with_base();
    let path = dir.path().to_string_lossy().into_owned();

    // Two commits, so the amended one has a parent to preserve.
    let base = repo.head().unwrap().peel_to_commit().unwrap();
    let tree = repo.find_tree(repo.index().unwrap().write_tree().unwrap()).unwrap();
    let identity = CommitIdentity { author: sig(0), committer: sig(0) };
    std::fs::write(dir.path().join("b.txt"), "two").unwrap();
    let mut index = repo.index().unwrap();
    index.add_path(std::path::Path::new("b.txt")).unwrap();
    index.write().unwrap();
    let tree2 = repo.find_tree(index.write_tree().unwrap()).unwrap();
    let second = repo
      .find_commit(create(&repo, &path, Some("HEAD"), &identity, "second", &tree2, &[&base]).unwrap())
      .unwrap();
    let _ = tree;

    let amended = amend_head(&repo, &path, &second, &identity, "reworded", &tree2).unwrap();
    let amended = repo.find_commit(amended).unwrap();

    assert_eq!(amended.message().unwrap(), "reworded");
    assert_eq!(amended.parent_id(0).unwrap(), base.id());
    assert_eq!(repo.head().unwrap().peel_to_commit().unwrap().id(), amended.id());
  }
}
