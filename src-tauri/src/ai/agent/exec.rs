//! Running the four tools against a real repository.
//!
//! Every path goes through [`super::tools::resolve_in_repo`] first, so the
//! boundary is enforced here rather than trusted to whatever the model asked
//! for. A tool that cannot be run returns an error string rather than failing
//! the turn: the model is told and can correct itself, which is the whole
//! reason refusals carry a sentence.

use std::path::{Path, PathBuf};

use serde_json::Value;

use super::tools::{self, PathRefusal};

/// How many bytes of a file to hand back.
///
/// A whole large file would spend the turn's budget on one read and crowd out
/// the actual work. Truncation is announced so the model knows it is seeing
/// part of something.
const MAX_READ_BYTES: usize = 60_000;

/// Entries listed before the rest are summarised.
const MAX_ENTRIES: usize = 200;

/// Runs one tool call inside `root`.
pub fn run_tool(root: &Path, name: &str, arguments: &Value) -> Result<String, String> {
  match name {
    "read_file" => read_file(root, arg_str(arguments, "path")?),
    "edit_file" => edit_file(
      root,
      arg_str(arguments, "path")?,
      arg_str(arguments, "contents")?,
    ),
    "list_directory" => list_directory(root, arg_str(arguments, "path").unwrap_or(".")),
    // `run_check` is deliberately not implemented here: which checks a project
    // has is the project's business, and inventing one would run a command the
    // user never configured. Reported rather than silently doing nothing.
    "run_check" => Err(
      "refused: GitWyrm doesn't have a check configured for this project yet, so there \
       is nothing to run."
        .into(),
    ),
    other => Err(format!("refused: {other} is not a tool this run can use")),
  }
}

fn arg_str<'a>(arguments: &'a Value, key: &str) -> Result<&'a str, String> {
  arguments
    .get(key)
    .and_then(Value::as_str)
    .ok_or_else(|| format!("refused: this needs a \"{key}\" value, as text"))
}

fn resolve(root: &Path, path: &str) -> Result<PathBuf, String> {
  tools::resolve_in_repo(root, path).map_err(|r: PathRefusal| r.message().to_string())
}

fn read_file(root: &Path, path: &str) -> Result<String, String> {
  let full = resolve(root, path)?;
  let bytes = std::fs::read(&full).map_err(|e| match e.kind() {
    std::io::ErrorKind::NotFound => format!("refused: {path} does not exist"),
    _ => format!("refused: {path} could not be read ({e})"),
  })?;

  // Binary files are refused rather than handed over as replacement
  // characters, which read as corruption and invite the model to "fix" them.
  let text = String::from_utf8(bytes)
    .map_err(|_| format!("refused: {path} is not a text file"))?;

  if text.len() > MAX_READ_BYTES {
    let cut = text
      .char_indices()
      .take_while(|(i, _)| *i < MAX_READ_BYTES)
      .last()
      .map(|(i, c)| i + c.len_utf8())
      .unwrap_or(0);
    return Ok(format!(
      "{}\n\n[... {} more bytes not shown ...]",
      &text[..cut],
      text.len() - cut
    ));
  }
  Ok(text)
}

fn edit_file(root: &Path, path: &str, contents: &str) -> Result<String, String> {
  let full = resolve(root, path)?;
  if let Some(parent) = full.parent() {
    std::fs::create_dir_all(parent)
      .map_err(|e| format!("refused: could not make the folder for {path} ({e})"))?;
  }
  let existed = full.exists();
  std::fs::write(&full, contents)
    .map_err(|e| format!("refused: {path} could not be written ({e})"))?;
  Ok(if existed {
    format!("Updated {path}")
  } else {
    format!("Created {path}")
  })
}

fn list_directory(root: &Path, path: &str) -> Result<String, String> {
  // "." is the repository root, which `resolve_in_repo` refuses as a file
  // path. Listing the root is legitimate, so it is handled before resolving.
  let full = if path == "." || path.is_empty() {
    root.to_path_buf()
  } else {
    resolve(root, path)?
  };

  let entries = std::fs::read_dir(&full).map_err(|e| match e.kind() {
    std::io::ErrorKind::NotFound => format!("refused: {path} does not exist"),
    _ => format!("refused: {path} could not be listed ({e})"),
  })?;

  let mut names: Vec<String> = Vec::new();
  for entry in entries.flatten() {
    let name = entry.file_name().to_string_lossy().into_owned();
    // `.git` is not listed at all: it is refused as a path, so showing it
    // would only invite a call that gets refused.
    if name.eq_ignore_ascii_case(".git") {
      continue;
    }
    let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
    names.push(if is_dir { format!("{name}/") } else { name });
  }
  names.sort();

  if names.is_empty() {
    return Ok(format!("{path} is empty"));
  }
  if names.len() > MAX_ENTRIES {
    let shown = names[..MAX_ENTRIES].join("\n");
    return Ok(format!(
      "{shown}\n[... {} more entries not shown ...]",
      names.len() - MAX_ENTRIES
    ));
  }
  Ok(names.join("\n"))
}

#[cfg(test)]
mod tests {
  use super::*;
  use serde_json::json;

  fn temp_repo(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("gitwyrm-exec-{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
  }

  #[test]
  fn writes_then_reads_a_file() {
    let root = temp_repo("roundtrip");
    let out = run_tool(&root, "edit_file", &json!({ "path": "a.txt", "contents": "hi" }));
    assert_eq!(out.unwrap(), "Created a.txt");
    let back = run_tool(&root, "read_file", &json!({ "path": "a.txt" })).unwrap();
    assert_eq!(back, "hi");
    // A second write reports an update, not a creation.
    let again = run_tool(&root, "edit_file", &json!({ "path": "a.txt", "contents": "yo" }));
    assert_eq!(again.unwrap(), "Updated a.txt");
    let _ = std::fs::remove_dir_all(&root);
  }

  #[test]
  fn creates_missing_folders_on_the_way() {
    let root = temp_repo("nested");
    run_tool(&root, "edit_file", &json!({ "path": "a/b/c.txt", "contents": "x" })).unwrap();
    assert!(root.join("a").join("b").join("c.txt").is_file());
    let _ = std::fs::remove_dir_all(&root);
  }

  #[test]
  fn a_path_outside_the_repo_is_refused() {
    let root = temp_repo("escape");
    let err = run_tool(&root, "read_file", &json!({ "path": "../../secrets" })).unwrap_err();
    assert!(err.starts_with("refused:"), "{err}");
    let _ = std::fs::remove_dir_all(&root);
  }

  #[test]
  fn git_internals_are_refused() {
    let root = temp_repo("gitdir");
    let err =
      run_tool(&root, "edit_file", &json!({ "path": ".git/config", "contents": "x" }))
        .unwrap_err();
    assert!(err.contains(".git"), "{err}");
    let _ = std::fs::remove_dir_all(&root);
  }

  #[test]
  fn a_missing_file_says_so_rather_than_erroring_out() {
    let root = temp_repo("missing");
    let err = run_tool(&root, "read_file", &json!({ "path": "nope.txt" })).unwrap_err();
    assert!(err.contains("does not exist"), "{err}");
    let _ = std::fs::remove_dir_all(&root);
  }

  #[test]
  fn listing_the_root_works_and_hides_git() {
    let root = temp_repo("listroot");
    std::fs::create_dir_all(root.join(".git")).unwrap();
    std::fs::write(root.join("readme.md"), "x").unwrap();
    let out = run_tool(&root, "list_directory", &json!({ "path": "." })).unwrap();
    assert!(out.contains("readme.md"));
    assert!(!out.contains(".git"), "the git folder must not be listed: {out}");
    let _ = std::fs::remove_dir_all(&root);
  }

  #[test]
  fn a_binary_file_is_refused_not_mangled() {
    let root = temp_repo("binary");
    std::fs::write(root.join("blob.bin"), [0xff, 0xfe, 0x00, 0x01]).unwrap();
    let err = run_tool(&root, "read_file", &json!({ "path": "blob.bin" })).unwrap_err();
    assert!(err.contains("not a text file"), "{err}");
    let _ = std::fs::remove_dir_all(&root);
  }

  #[test]
  fn a_long_file_is_truncated_and_says_so() {
    let root = temp_repo("long");
    let big = "x".repeat(MAX_READ_BYTES + 500);
    std::fs::write(root.join("big.txt"), &big).unwrap();
    let out = run_tool(&root, "read_file", &json!({ "path": "big.txt" })).unwrap();
    assert!(out.contains("not shown"), "truncation must be announced");
    assert!(out.len() < big.len());
    let _ = std::fs::remove_dir_all(&root);
  }

  #[test]
  fn a_missing_argument_is_reported_in_plain_words() {
    let root = temp_repo("args");
    let err = run_tool(&root, "read_file", &json!({})).unwrap_err();
    assert!(err.contains("\"path\""), "{err}");
    let _ = std::fs::remove_dir_all(&root);
  }

  #[test]
  fn an_unknown_tool_is_refused() {
    let root = temp_repo("unknown");
    let err = run_tool(&root, "bash", &json!({ "cmd": "ls" })).unwrap_err();
    assert!(err.contains("not a tool"), "{err}");
    let _ = std::fs::remove_dir_all(&root);
  }

  #[test]
  fn run_check_says_there_is_nothing_configured() {
    // Better than silently doing nothing: the model would otherwise believe it
    // had verified its work.
    let root = temp_repo("check");
    let err = run_tool(&root, "run_check", &json!({ "name": "typecheck" })).unwrap_err();
    assert!(err.contains("nothing to run"), "{err}");
    let _ = std::fs::remove_dir_all(&root);
  }
}
