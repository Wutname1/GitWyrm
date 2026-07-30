//! End-to-end exercise of branch-to-change links against a real repository.
//!
//! The unit tests cover trailer parsing and config storage separately. This
//! drives the round trip the UI performs -- link a branch, commit with the
//! trailer, read it back off the commit -- so a change to either half that
//! breaks the other shows up here.

use std::process::Command;

use gitwyrm_lib::{git_spec_link as spec_link, git_trailers as trailers};

fn git(dir: &std::path::Path, args: &[&str]) -> String {
  let out = Command::new("git")
    .current_dir(dir)
    .args(args)
    .output()
    .expect("run git");
  assert!(
    out.status.success(),
    "git {args:?} failed: {}",
    String::from_utf8_lossy(&out.stderr)
  );
  String::from_utf8_lossy(&out.stdout).trim().to_string()
}

/// A repository with one commit, so HEAD exists and branches can be made.
fn repo() -> (tempfile::TempDir, String) {
  let dir = tempfile::tempdir().unwrap();
  let path = dir.path();
  git(path, &["init", "-q", "."]);
  git(path, &["config", "user.email", "t@example.com"]);
  git(path, &["config", "user.name", "Test"]);
  std::fs::write(path.join("seed.txt"), "seed").unwrap();
  git(path, &["add", "-A"]);
  git(path, &["commit", "-q", "-m", "new: seed"]);
  let branch = git(path, &["rev-parse", "--abbrev-ref", "HEAD"]);
  (dir, branch)
}

/// Commit a file with `message`, the way `create_commit` composes it.
fn commit(dir: &std::path::Path, name: &str, message: &str) {
  std::fs::write(dir.join(name), name).unwrap();
  git(dir, &["add", "-A"]);
  git(dir, &["commit", "-q", "-m", message]);
}

#[test]
fn a_linked_branch_round_trips_through_git_config() {
  let (dir, branch) = repo();
  let path = dir.path().to_str().unwrap();

  assert!(spec_link::resolve(path, &branch, None).unwrap().is_none());

  spec_link::set_link(path, &branch, "add-thing").unwrap();
  let link = spec_link::resolve(path, &branch, None).unwrap().unwrap();
  assert_eq!(link.change_id, "add-thing");

  // The link is real git config, readable by git itself -- not a private
  // sidecar file that a clone or another tool would miss.
  let stored = git(
    dir.path(),
    &["config", "--local", "--get", &format!("branch.{branch}.gitwyrm-spec")],
  );
  assert_eq!(stored, "add-thing");
}

#[test]
fn a_committed_trailer_is_readable_off_the_commit() {
  let (dir, _branch) = repo();
  let message = trailers::upsert("new: a thing\n\nSome detail.", trailers::SPEC_KEY, "add-thing");
  commit(dir.path(), "a.txt", &message);

  // Read the stored message back the way the graph walk does.
  let body = git(dir.path(), &["log", "-1", "--format=%B"]);
  assert_eq!(trailers::spec_id(&body).as_deref(), Some("add-thing"));

  // And the way git itself parses trailers, which proves the format is the
  // real thing rather than something only our parser accepts.
  let via_git = git(
    dir.path(),
    &["log", "-1", "--format=%(trailers:key=Spec,valueonly)"],
  );
  assert_eq!(via_git.trim(), "add-thing");
}

#[test]
fn unlinking_keeps_existing_commit_trailers() {
  let (dir, branch) = repo();
  let path = dir.path().to_str().unwrap();
  spec_link::set_link(path, &branch, "add-thing").unwrap();

  let message = trailers::upsert("new: a thing", trailers::SPEC_KEY, "add-thing");
  commit(dir.path(), "a.txt", &message);
  spec_link::clear_link(path, &branch).unwrap();

  // The explicit link is gone...
  assert!(spec_link::explicit_link(path, &branch).is_none());
  // ...but history was not rewritten, so the commit keeps its chip.
  let body = git(dir.path(), &["log", "-1", "--format=%B"]);
  assert_eq!(trailers::spec_id(&body).as_deref(), Some("add-thing"));
}

#[test]
fn a_feature_branch_does_not_inherit_its_base_link() {
  let (dir, base) = repo();
  let path = dir.path().to_str().unwrap();

  let message = trailers::upsert("new: base work", trailers::SPEC_KEY, "base-change");
  commit(dir.path(), "base.txt", &message);

  git(dir.path(), &["checkout", "-q", "-b", "feature"]);
  commit(dir.path(), "feature.txt", "new: unrelated feature work");

  // Excluding the base's history is what keeps the feature branch honest.
  assert!(spec_link::resolve(path, "feature", Some(&base))
    .unwrap()
    .is_none());
  // Without a base to exclude, the base's trailer would leak in.
  assert!(spec_link::resolve(path, "feature", None).unwrap().is_some());
}

#[test]
fn an_amended_message_keeps_exactly_one_trailer() {
  let (dir, _branch) = repo();
  let first = trailers::upsert("new: a thing", trailers::SPEC_KEY, "add-thing");
  commit(dir.path(), "a.txt", &first);

  // Amending re-runs upsert over a message that already carries the trailer,
  // which is where a naive append would duplicate it.
  let amended = trailers::upsert(&first, trailers::SPEC_KEY, "add-thing");
  git(dir.path(), &["commit", "-q", "--amend", "-m", &amended]);

  let body = git(dir.path(), &["log", "-1", "--format=%B"]);
  assert_eq!(body.matches("Spec:").count(), 1);
  assert_eq!(trailers::spec_id(&body).as_deref(), Some("add-thing"));
}
