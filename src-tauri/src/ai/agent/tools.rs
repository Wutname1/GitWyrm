//! The bounded tool set, and the repository boundary.
//!
//! Four tools: read a file, edit a file, list a directory, run a project
//! check. Nothing else exists, so nothing else can be called. Every path is
//! resolved and checked against the repository root before any I/O happens.
//!
//! The boundary is enforced here rather than trusted to the provider. A CLI is
//! a model transport, not a trust boundary: whatever a given tool would permit
//! on its own, a path outside the repository is refused by our code.

use std::path::{Component, Path, PathBuf};

use serde_json::{json, Value};

use super::transport::ToolSchema;

/// Why a path was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathRefusal {
  /// Resolved outside the repository.
  OutsideRepo,
  /// Absolute, or otherwise not a plain relative path inside the repo.
  NotRelative,
  /// Inside `.git`. Editing git's own store is not a file edit, it is a
  /// history rewrite by other means.
  GitInternals,
}

impl PathRefusal {
  /// What the model is told, so it can correct rather than retry blindly.
  pub fn message(self) -> &'static str {
    match self {
      PathRefusal::OutsideRepo => "refused: that path is outside this repository",
      PathRefusal::NotRelative => {
        "refused: use a path relative to the repository root, not an absolute one"
      }
      PathRefusal::GitInternals => "refused: the .git folder is not editable",
    }
  }
}

/// Resolves `candidate` against `root`, refusing anything that escapes.
///
/// Lexical normalisation first (so `a/../../b` is caught even when the path
/// does not exist yet, which is the create-a-file case), then a canonicalised
/// re-check when the target does exist -- that second pass is what catches a
/// symlink pointing out of the repository, which no amount of string
/// inspection would.
pub fn resolve_in_repo(root: &Path, candidate: &str) -> Result<PathBuf, PathRefusal> {
  let rel = Path::new(candidate);
  if rel.is_absolute() || rel.has_root() {
    return Err(PathRefusal::NotRelative);
  }
  // A Windows path like `C:foo` or a UNC prefix is not a plain relative path.
  if rel.components().any(|c| matches!(c, Component::Prefix(_) | Component::RootDir)) {
    return Err(PathRefusal::NotRelative);
  }

  let mut out = PathBuf::new();
  for comp in rel.components() {
    match comp {
      Component::CurDir => {}
      Component::ParentDir => {
        // Refuse rather than clamp: silently turning `../x` into `x` would
        // write somewhere the model did not ask for.
        if !out.pop() {
          return Err(PathRefusal::OutsideRepo);
        }
      }
      Component::Normal(part) => {
        if part.eq_ignore_ascii_case(".git") {
          return Err(PathRefusal::GitInternals);
        }
        out.push(part);
      }
      Component::Prefix(_) | Component::RootDir => return Err(PathRefusal::NotRelative),
    }
  }

  if out.as_os_str().is_empty() {
    return Err(PathRefusal::OutsideRepo);
  }

  let full = root.join(&out);

  // Existing targets get the real check: canonicalize follows symlinks, so a
  // link inside the repo pointing outside it is caught here and nowhere else.
  if let (Ok(real), Ok(real_root)) = (full.canonicalize(), root.canonicalize()) {
    if !real.starts_with(&real_root) {
      return Err(PathRefusal::OutsideRepo);
    }
  }

  Ok(full)
}

/// The tools offered to the model, and nothing else.
pub fn tool_schemas() -> Vec<ToolSchema> {
  vec![
    ToolSchema {
      name: "read_file".into(),
      description: "Read a file from the repository.".into(),
      parameters: json!({
        "type": "object",
        "properties": {
          "path": { "type": "string", "description": "Path relative to the repository root" }
        },
        "required": ["path"]
      }),
    },
    ToolSchema {
      name: "edit_file".into(),
      description: "Replace the whole contents of a file, creating it if needed.".into(),
      parameters: json!({
        "type": "object",
        "properties": {
          "path": { "type": "string", "description": "Path relative to the repository root" },
          "contents": { "type": "string" }
        },
        "required": ["path", "contents"]
      }),
    },
    ToolSchema {
      name: "list_directory".into(),
      description: "List the files in a directory of the repository.".into(),
      parameters: json!({
        "type": "object",
        "properties": {
          "path": { "type": "string", "description": "Path relative to the repository root" }
        },
        "required": ["path"]
      }),
    },
    ToolSchema {
      name: "run_check".into(),
      description: "Run one of this project's own checks and report the result.".into(),
      parameters: json!({
        "type": "object",
        "properties": {
          "name": { "type": "string", "description": "Which configured check to run" }
        },
        "required": ["name"]
      }),
    },
  ]
}

/// Whether `name` is one of the four. Anything else is raised as a gate rather
/// than executed, so an unknown tool cannot become an arbitrary command.
pub fn is_known_tool(name: &str) -> bool {
  matches!(name, "read_file" | "edit_file" | "list_directory" | "run_check")
}

#[cfg(test)]
mod tests {
  use super::*;

  fn root() -> PathBuf {
    std::env::temp_dir().join("gitwyrm-tool-tests")
  }

  #[test]
  fn a_plain_relative_path_resolves() {
    let r = root();
    let p = resolve_in_repo(&r, "src/main.rs").unwrap();
    assert!(p.starts_with(&r));
    assert!(p.ends_with("main.rs"));
  }

  #[test]
  fn parent_traversal_is_refused() {
    assert_eq!(resolve_in_repo(&root(), "../secrets"), Err(PathRefusal::OutsideRepo));
    assert_eq!(resolve_in_repo(&root(), "a/../../secrets"), Err(PathRefusal::OutsideRepo));
  }

  #[test]
  fn traversal_that_returns_inside_is_allowed() {
    // `a/../b` never leaves the repository, so refusing it would block
    // legitimate paths a model may well produce.
    let p = resolve_in_repo(&root(), "a/../b.txt").unwrap();
    assert!(p.ends_with("b.txt"));
  }

  #[test]
  fn absolute_paths_are_refused() {
    assert_eq!(resolve_in_repo(&root(), "/etc/passwd"), Err(PathRefusal::NotRelative));
    #[cfg(windows)]
    assert_eq!(resolve_in_repo(&root(), r"C:\Windows\System32"), Err(PathRefusal::NotRelative));
  }

  #[test]
  fn git_internals_are_refused() {
    assert_eq!(resolve_in_repo(&root(), ".git/config"), Err(PathRefusal::GitInternals));
    // Case-insensitively, because Windows would treat these as the same file.
    assert_eq!(resolve_in_repo(&root(), ".GIT/config"), Err(PathRefusal::GitInternals));
    assert_eq!(resolve_in_repo(&root(), "sub/.git/hooks"), Err(PathRefusal::GitInternals));
  }

  #[test]
  fn the_root_itself_is_not_a_file() {
    assert_eq!(resolve_in_repo(&root(), ""), Err(PathRefusal::OutsideRepo));
    assert_eq!(resolve_in_repo(&root(), "."), Err(PathRefusal::OutsideRepo));
  }

  #[test]
  fn only_the_four_tools_are_known() {
    for name in ["read_file", "edit_file", "list_directory", "run_check"] {
      assert!(is_known_tool(name), "{name} should be known");
    }
    for name in ["bash", "shell", "exec", "run_command", "fetch", "push", "git"] {
      assert!(!is_known_tool(name), "{name} must not be callable");
    }
  }

  #[test]
  fn the_schema_list_matches_what_is_known() {
    // Guards against a schema being offered that the executor would refuse, or
    // a tool being executable that was never advertised.
    for s in tool_schemas() {
      assert!(is_known_tool(&s.name), "{} is offered but not known", s.name);
    }
    assert_eq!(tool_schemas().len(), 4);
  }

  #[test]
  fn every_refusal_tells_the_model_what_to_do() {
    for r in [PathRefusal::OutsideRepo, PathRefusal::NotRelative, PathRefusal::GitInternals] {
      assert!(r.message().starts_with("refused:"), "{:?}", r);
    }
  }

  #[test]
  fn a_symlink_out_of_the_repo_is_refused() {
    // The lexical check cannot catch this; only canonicalize can.
    let base = std::env::temp_dir().join("gitwyrm-symlink-test");
    let repo = base.join("repo");
    let outside = base.join("outside");
    let _ = std::fs::create_dir_all(&repo);
    let _ = std::fs::create_dir_all(&outside);
    let _ = std::fs::write(outside.join("secret.txt"), b"x");

    let link = repo.join("escape");
    let made = {
      #[cfg(windows)]
      {
        std::os::windows::fs::symlink_dir(&outside, &link).is_ok()
      }
      #[cfg(not(windows))]
      {
        std::os::unix::fs::symlink(&outside, &link).is_ok()
      }
    };

    if made {
      assert_eq!(
        resolve_in_repo(&repo, "escape/secret.txt"),
        Err(PathRefusal::OutsideRepo),
        "a symlink out of the repo must be refused"
      );
    } else {
      // Creating a symlink on Windows needs elevation (error 1314), so on an
      // ordinary developer machine this assertion never runs. Said out loud
      // rather than passing quietly, because a green test here would otherwise
      // imply the canonicalize guard has been exercised when it has not.
      eprintln!(
        "NOTE: symlink escape not exercised -- could not create a symlink in this environment"
      );
    }

    let _ = std::fs::remove_dir_all(&base);
  }
}
