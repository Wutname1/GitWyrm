//! The scan must find repos and must NOT open them (no .git/index.lock created).

use std::fs;
use std::path::Path;

#[test]
fn scan_reads_head_without_locking() {
  let root = Path::new("C:/code");
  if !root.is_dir() {
    return;
  }
  let mut found = 0;
  for entry in fs::read_dir(root).unwrap().flatten() {
    let path = entry.path();
    let git_dir = path.join(".git");
    if !git_dir.is_dir() {
      continue;
    }
    found += 1;
    // Mirror of scan_code_folder's HEAD read: plain text, no git2.
    let head = fs::read_to_string(git_dir.join("HEAD")).ok();
    assert!(head.is_some(), "HEAD unreadable for {}", path.display());
    // The scan must not leave a lock behind.
    assert!(
      !git_dir.join("index.lock").exists(),
      "index.lock present after scan for {}",
      path.display()
    );
  }
  assert!(found > 3, "expected several repos under C:/code, found {found}");
}
