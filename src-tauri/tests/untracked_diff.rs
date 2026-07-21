//! An untracked file must produce a full add-diff, the same way the status
//! sidebar counts it. Regression guard: without `recurse_untracked_dirs`,
//! `show_untracked_content` yields zero lines and the diff pane looks empty.

use git2::{DiffOptions, Repository};

fn temp_repo_with_untracked() -> (tempfile::TempDir, Repository) {
  let dir = tempfile::tempdir().unwrap();
  let repo = Repository::init(dir.path()).unwrap();

  // A committed file so the repo has a HEAD and the dir isn't collapsed.
  std::fs::create_dir_all(dir.path().join("docs")).unwrap();
  std::fs::write(dir.path().join("docs/tracked.txt"), "one\n").unwrap();
  {
    let mut index = repo.index().unwrap();
    index.add_path(std::path::Path::new("docs/tracked.txt")).unwrap();
    index.write().unwrap();
    let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
    let sig = git2::Signature::now("t", "t@example.com").unwrap();
    repo.commit(Some("HEAD"), &sig, &sig, "init", &tree, &[]).unwrap();
  }

  // The untracked file under test: 5 lines, nested in a dir with tracked content.
  std::fs::write(dir.path().join("docs/new.html"), "a\nb\nc\nd\ne\n").unwrap();

  (dir, repo)
}

fn added_lines(repo: &Repository, opts: &mut DiffOptions) -> usize {
  let diff = repo.diff_index_to_workdir(None, Some(opts)).unwrap();
  let mut n = 0usize;
  diff
    .foreach(
      &mut |_, _| true,
      None,
      None,
      Some(&mut |_, _, line| {
        if line.origin() == '+' {
          n += 1;
        }
        true
      }),
    )
    .unwrap();
  n
}

#[test]
fn untracked_file_diff_yields_its_lines() {
  let (_dir, repo) = temp_repo_with_untracked();
  let mut opts = DiffOptions::new();
  opts
    .pathspec("docs/new.html")
    .include_untracked(true)
    .recurse_untracked_dirs(true)
    .show_untracked_content(true)
    .context_lines(3);
  assert_eq!(added_lines(&repo, &mut opts), 5, "untracked file should diff as 5 added lines");
}

#[test]
fn without_recurse_untracked_dirs_the_content_is_missing() {
  let (_dir, repo) = temp_repo_with_untracked();
  let mut opts = DiffOptions::new();
  opts
    .pathspec("docs/new.html")
    .include_untracked(true)
    .show_untracked_content(true)
    .context_lines(3);
  // Documents the old behavior this fix corrects.
  assert_eq!(added_lines(&repo, &mut opts), 0);
}
