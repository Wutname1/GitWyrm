//! Partial staging: stage/unstage/discard individual hunks or lines.
//!
//! Strategy: ask git itself for the exact unified diff of the file (so we keep
//! byte-accurate context, "\ No newline at end of file" markers, and mode
//! lines), filter it down to the caller's selected lines, recompute the hunk
//! `@@` counts, then feed the rebuilt patch to `git apply`:
//!   - stage selection:   apply to the index          (`--cached`)
//!   - unstage selection: reverse-apply to the index  (`--cached --reverse`)
//!   - discard selection: reverse-apply to the workdir (`--reverse`)
//!
//! Selecting a whole hunk is just "every changed line in that hunk", so hunks
//! and lines share one code path.

use serde::Deserialize;
use specta::Type;
use tauri::State;

use crate::error::AppError;
use crate::git::shell::{run_git, run_git_stdin};
use crate::state::RepoManager;

/// Which side of the working tree the selection came from. Mirrors the diff the
/// frontend rendered so line numbers line up.
#[derive(Debug, Clone, Copy, Deserialize, Type, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum PatchTarget {
  /// Unstaged changes (index -> workdir). Stage or discard these.
  Unstaged,
  /// Staged changes (HEAD -> index). Unstage these.
  Staged,
}

/// One changed line the caller selected, identified by its diff line numbers.
/// Added lines carry `new_no`; removed lines carry `old_no`.
#[derive(Debug, Clone, Deserialize, Type)]
pub struct SelectedLine {
  pub hunk_index: u32,
  pub old_no: Option<u32>,
  pub new_no: Option<u32>,
}

impl SelectedLine {
  fn matches(&self, hunk_index: u32, sign: u8, old_no: Option<u32>, new_no: Option<u32>) -> bool {
    if self.hunk_index != hunk_index {
      return false;
    }
    match sign {
      b'+' => self.new_no.is_some() && self.new_no == new_no,
      b'-' => self.old_no.is_some() && self.old_no == old_no,
      _ => false,
    }
  }
}

/// A parsed hunk: its header line and the body lines (each with leading sign).
struct RawHunk {
  header: String,
  old_start: u32,
  new_start: u32,
  lines: Vec<String>,
}

/// Everything before the first `@@` in a file's diff (the `diff --git`, index,
/// `---`/`+++`, and any mode lines), plus the parsed hunks.
struct ParsedDiff {
  preamble: Vec<String>,
  hunks: Vec<RawHunk>,
}

/// The smallest safe slice of a diff that can be moved to another commit.
/// Adjacent additions/deletions stay together because splitting a replacement
/// in the middle can produce a patch that applies in the wrong place.
pub(crate) struct LogicalPatchUnit {
  pub selection: Vec<SelectedLine>,
  pub preview: String,
}

fn parse_hunk_header(header: &str) -> Option<(u32, u32)> {
  // Form: @@ -old_start[,old_len] +new_start[,new_len] @@ optional section
  let body = header.strip_prefix("@@ ")?;
  let end = body.find(" @@")?;
  let ranges = &body[..end];
  let mut parts = ranges.split(' ');
  let old = parts.next()?.strip_prefix('-')?;
  let new = parts.next()?.strip_prefix('+')?;
  let old_start: u32 = old.split(',').next()?.parse().ok()?;
  let new_start: u32 = new.split(',').next()?.parse().ok()?;
  Some((old_start, new_start))
}

/// Split a single-file unified diff into preamble + hunks.
fn parse_diff(text: &str) -> Result<ParsedDiff, AppError> {
  let mut preamble = Vec::new();
  let mut hunks: Vec<RawHunk> = Vec::new();
  let mut seen_hunk = false;

  for line in text.split_inclusive('\n') {
    let line = line.to_string();
    if line.starts_with("@@ ") {
      seen_hunk = true;
      let (old_start, new_start) = parse_hunk_header(&line)
        .ok_or_else(|| AppError::Other(format!("unparseable hunk header: {}", line.trim_end())))?;
      hunks.push(RawHunk { header: line, old_start, new_start, lines: Vec::new() });
    } else if !seen_hunk {
      preamble.push(line);
    } else {
      hunks.last_mut().unwrap().lines.push(line);
    }
  }

  Ok(ParsedDiff { preamble, hunks })
}

/// Break a single-file patch into independently stageable change blocks. This
/// is shared with AI commit splitting so changes from one file can belong to
/// different commits without asking the model to write or edit patch syntax.
pub(crate) fn logical_patch_units(raw: &str) -> Result<Vec<LogicalPatchUnit>, AppError> {
  let parsed = parse_diff(raw)?;
  let mut units = Vec::new();

  for (hunk_index, hunk) in parsed.hunks.iter().enumerate() {
    let mut old_no = hunk.old_start;
    let mut new_no = hunk.new_start;
    let mut selection = Vec::new();
    let mut preview = String::new();

    let flush = |selection: &mut Vec<SelectedLine>,
                 preview: &mut String,
                 units: &mut Vec<LogicalPatchUnit>| {
      if selection.is_empty() {
        return;
      }
      units.push(LogicalPatchUnit {
        selection: std::mem::take(selection),
        preview: format!("{}{}", hunk.header, std::mem::take(preview)),
      });
    };

    for raw_line in &hunk.lines {
      let sign = raw_line.as_bytes().first().copied().unwrap_or(b' ');
      match sign {
        b'+' => {
          selection.push(SelectedLine {
            hunk_index: hunk_index as u32,
            old_no: None,
            new_no: Some(new_no),
          });
          preview.push_str(raw_line);
          new_no += 1;
        }
        b'-' => {
          selection.push(SelectedLine {
            hunk_index: hunk_index as u32,
            old_no: Some(old_no),
            new_no: None,
          });
          preview.push_str(raw_line);
          old_no += 1;
        }
        b'\\' => {
          if !selection.is_empty() {
            preview.push_str(raw_line);
          }
        }
        _ => {
          flush(&mut selection, &mut preview, &mut units);
          old_no += 1;
          new_no += 1;
        }
      }
    }
    flush(&mut selection, &mut preview, &mut units);
  }

  Ok(units)
}

/// Validate that within each contiguous run of changed lines (a maximal block
/// of consecutive +/- lines with no context between them), the selection is
/// all-or-nothing. A partially-selected contiguous replace block cannot be
/// represented as a patch that git applies to the intended position — the
/// added lines would land after the unselected deletions. The frontend expands
/// such selections to whole blocks; this is the backend backstop.
fn validate_contiguous_blocks(
  hunk: &RawHunk,
  hunk_index: u32,
  selection: &[SelectedLine],
) -> Result<(), AppError> {
  let mut old_no = hunk.old_start;
  let mut new_no = hunk.new_start;
  // Track selection state within the current contiguous change run.
  let mut run_len = 0u32;
  let mut run_selected = 0u32;

  let flush = |len: u32, sel: u32| -> Result<(), AppError> {
    if len > 1 && sel > 0 && sel < len {
      return Err(AppError::Other(
        "cannot stage part of a contiguous change block; select the whole block".into(),
      ));
    }
    Ok(())
  };

  for raw in &hunk.lines {
    let sign = raw.as_bytes().first().copied().unwrap_or(b' ');
    match sign {
      b'+' => {
        let sel = selection.iter().any(|s| s.matches(hunk_index, b'+', None, Some(new_no)));
        run_len += 1;
        if sel {
          run_selected += 1;
        }
        new_no += 1;
      }
      b'-' => {
        let sel = selection.iter().any(|s| s.matches(hunk_index, b'-', Some(old_no), None));
        run_len += 1;
        if sel {
          run_selected += 1;
        }
        old_no += 1;
      }
      b'\\' => {}
      _ => {
        // Context line ends the run.
        flush(run_len, run_selected)?;
        run_len = 0;
        run_selected = 0;
        old_no += 1;
        new_no += 1;
      }
    }
  }
  flush(run_len, run_selected)
}

/// Rebuild a hunk keeping only selected changes; unselected changes are demoted
/// so the patch still applies. Recomputes the `@@` counts. Returns None if the
/// hunk ends up with no real change (skip it entirely).
///
/// `reverse` selects which side the patch must line up against when applied:
///   - forward (stage/discard): the patch's PRE-image is the old side. Unselected
///     deletions become context; unselected additions are dropped.
///   - reverse (unstage): `git apply --reverse` matches the patch's POST-image
///     (new side) against the index. So unselected additions become context and
///     unselected deletions are dropped instead.
fn rebuild_hunk(
  hunk: &RawHunk,
  hunk_index: u32,
  selection: &[SelectedLine],
  reverse: bool,
) -> Option<String> {
  let mut old_no = hunk.old_start;
  let mut new_no = hunk.new_start;
  let mut old_count = 0u32;
  let mut new_count = 0u32;
  let mut body: Vec<String> = Vec::new();
  let mut has_change = false;

  for raw in &hunk.lines {
    let sign = raw.as_bytes().first().copied().unwrap_or(b' ');
    match sign {
      b'\\' => {
        // "\ No newline at end of file" annotates the line above; keep as-is.
        body.push(raw.clone());
        continue;
      }
      b' ' => {
        body.push(raw.clone());
        old_no += 1;
        new_no += 1;
        old_count += 1;
        new_count += 1;
      }
      b'+' => {
        let selected = selection.iter().any(|s| s.matches(hunk_index, b'+', None, Some(new_no)));
        if selected {
          body.push(raw.clone());
          new_count += 1;
          has_change = true;
        } else if reverse {
          // Reverse: unselected addition exists on both the pre (index) and the
          // post side of the trimmed patch, so keep it as context.
          let mut ctx = raw.clone();
          ctx.replace_range(0..1, " ");
          body.push(ctx);
          old_count += 1;
          new_count += 1;
        }
        // Forward: unselected addition dropped (absent from the target).
        new_no += 1;
      }
      b'-' => {
        let selected = selection.iter().any(|s| s.matches(hunk_index, b'-', Some(old_no), None));
        if selected {
          body.push(raw.clone());
          old_count += 1;
          has_change = true;
        } else if !reverse {
          // Forward: unselected deletion stays as context so offsets hold.
          let mut ctx = raw.clone();
          ctx.replace_range(0..1, " ");
          body.push(ctx);
          old_count += 1;
          new_count += 1;
        }
        // Reverse: unselected deletion dropped (absent from the index side).
        old_no += 1;
      }
      _ => body.push(raw.clone()),
    }
  }

  if !has_change {
    return None;
  }

  let header = format!(
    "@@ -{},{} +{},{} @@\n",
    hunk.old_start, old_count, hunk.new_start, new_count
  );
  let mut out = header;
  for l in body {
    out.push_str(&l);
  }
  Some(out)
}

/// Build a patch for `path` limited to `selection`. `diff_args` is the git diff
/// invocation that produces the source diff (staged vs unstaged).
fn build_patch(
  repo_path: &str,
  _path: &str,
  diff_args: &[&str],
  selection: &[SelectedLine],
  reverse: bool,
) -> Result<String, AppError> {
  let raw = run_git(Some(repo_path), diff_args)?.stdout;
  if raw.trim().is_empty() {
    return Err(AppError::Other("no changes found for this file".into()));
  }
  // Guard: we only ever build single-file patches. More than one `diff --git`
  // header means the caller passed a directory/glob, which parse_diff would
  // mis-fold into hunk bodies.
  if raw.match_indices("diff --git ").filter(|(i, _)| *i == 0 || raw.as_bytes()[i - 1] == b'\n').count() > 1 {
    return Err(AppError::Other("expected a single-file diff".into()));
  }
  build_patch_from_raw(&raw, selection, reverse)
}

/// Rebuild an already-captured single-file patch with only `selection`. The AI
/// commit workflow uses the same tested patch filtering as manual line staging.
pub(crate) fn build_patch_from_raw(
  raw: &str,
  selection: &[SelectedLine],
  reverse: bool,
) -> Result<String, AppError> {
  let parsed = parse_diff(&raw)?;

  let mut patch = String::new();
  for l in &parsed.preamble {
    patch.push_str(l);
  }

  let mut any = false;
  for (idx, hunk) in parsed.hunks.iter().enumerate() {
    validate_contiguous_blocks(hunk, idx as u32, selection)?;
    if let Some(rebuilt) = rebuild_hunk(hunk, idx as u32, selection, reverse) {
      patch.push_str(&rebuilt);
      any = true;
    }
  }

  if !any {
    return Err(AppError::Other("selection produced no applicable changes".into()));
  }
  Ok(patch)
}

fn diff_args_for(target: PatchTarget, path: &str) -> Vec<String> {
  // The frontend renders line numbers from a libgit2 diff (get_file_diff), which
  // uses the Myers algorithm with the indent heuristic OFF and honors no user
  // diff config. We parse git-CLI output here, so we must force the CLI to the
  // SAME anchoring or the caller's selected line numbers map to different
  // physical lines (silently staging/discarding the wrong change). Pin the
  // algorithm and disable the indent heuristic so both engines agree.
  let mut args: Vec<String> = vec![
    "-c".into(),
    "diff.indentHeuristic=false".into(),
    "-c".into(),
    "diff.algorithm=myers".into(),
    "diff".into(),
    "--no-color".into(),
    "--diff-algorithm=myers".into(),
    "-U3".into(),
  ];
  if target == PatchTarget::Staged {
    args.push("--cached".into());
  }
  args.push("--".into());
  args.push(path.into());
  args
}

/// Common entry: build the patch and apply it with the given `apply_args`.
/// `apply_args` is owned so it can cross into the blocking task.
async fn apply_selection(
  manager: State<'_, RepoManager>,
  repo_id: String,
  path: String,
  target: PatchTarget,
  selection: Vec<SelectedLine>,
  apply_args: Vec<&'static str>,
  reverse: bool,
) -> Result<(), AppError> {
  let open = manager.get(&repo_id)?;
  let repo_path = open.path.to_string_lossy().into_owned();
  tauri::async_runtime::spawn_blocking(move || {
    let dargs = diff_args_for(target, &path);
    let dargs_ref: Vec<&str> = dargs.iter().map(String::as_str).collect();
    let patch = build_patch(&repo_path, &path, &dargs_ref, &selection, reverse)?;
    run_git_stdin(Some(&repo_path), &apply_args, patch.as_bytes())?;
    Ok(())
  })
  .await
  .map_err(|e| AppError::Other(e.to_string()))?
}

#[tauri::command]
#[specta::specta]
pub async fn stage_lines(
  manager: State<'_, RepoManager>,
  repo_id: String,
  path: String,
  selection: Vec<SelectedLine>,
) -> Result<(), AppError> {
  apply_selection(
    manager,
    repo_id,
    path,
    PatchTarget::Unstaged,
    selection,
    vec!["apply", "--cached", "-"],
    false,
  )
  .await
}

#[tauri::command]
#[specta::specta]
pub async fn unstage_lines(
  manager: State<'_, RepoManager>,
  repo_id: String,
  path: String,
  selection: Vec<SelectedLine>,
) -> Result<(), AppError> {
  apply_selection(
    manager,
    repo_id,
    path,
    PatchTarget::Staged,
    selection,
    vec!["apply", "--cached", "--reverse", "-"],
    true,
  )
  .await
}

#[tauri::command]
#[specta::specta]
pub async fn discard_lines(
  manager: State<'_, RepoManager>,
  repo_id: String,
  path: String,
  selection: Vec<SelectedLine>,
) -> Result<(), AppError> {
  apply_selection(
    manager,
    repo_id,
    path,
    PatchTarget::Unstaged,
    selection,
    vec!["apply", "--reverse", "-"],
    true,
  )
  .await
}

#[cfg(test)]
mod tests {
  use super::*;

  fn sel(hunk: u32, old: Option<u32>, new: Option<u32>) -> SelectedLine {
    SelectedLine { hunk_index: hunk, old_no: old, new_no: new }
  }

  const SAMPLE: &str = "diff --git a/f.txt b/f.txt\nindex 111..222 100644\n--- a/f.txt\n+++ b/f.txt\n@@ -1,4 +1,4 @@\n one\n-two\n-three\n+TWO\n+THREE\n four\n";

  #[test]
  fn parses_preamble_and_hunks() {
    let d = parse_diff(SAMPLE).unwrap();
    assert_eq!(d.preamble.len(), 4);
    assert_eq!(d.hunks.len(), 1);
    assert_eq!(d.hunks[0].old_start, 1);
    assert_eq!(d.hunks[0].new_start, 1);
    assert_eq!(d.hunks[0].lines.len(), 6);
  }

  #[test]
  fn select_one_addition_and_its_deletion() {
    let d = parse_diff(SAMPLE).unwrap();
    // Layout: ctx one(new1), -two(old2), -three(old3), +TWO(new2), +THREE(new3), ctx four.
    // Select removing "two" (old 2) and adding "TWO" (new 2).
    let selection = vec![sel(0, Some(2), None), sel(0, None, Some(2))];
    let out = rebuild_hunk(&d.hunks[0], 0, &selection, false).unwrap();
    // "three"->"THREE" not selected: -three kept as context, +THREE dropped.
    assert!(out.contains("-two"), "out: {out}");
    assert!(out.contains("+TWO"), "out: {out}");
    assert!(out.contains(" three"), "out: {out}");
    assert!(!out.contains("+THREE"), "out: {out}");
    assert!(!out.contains("-three"), "out: {out}");
    // old_count: one, two, three, four = 4 ; new_count: one, TWO, three, four = 4
    assert!(out.starts_with("@@ -1,4 +1,4 @@\n"), "header was: {}", out.lines().next().unwrap());
  }

  #[test]
  fn no_selection_yields_none() {
    let d = parse_diff(SAMPLE).unwrap();
    assert!(rebuild_hunk(&d.hunks[0], 0, &[], false).is_none());
  }

  #[test]
  fn select_only_addition_recomputes_counts() {
    // Pure addition hunk.
    let text = "--- a/f\n+++ b/f\n@@ -1,2 +1,3 @@\n a\n+b\n c\n";
    let d = parse_diff(text).unwrap();
    let out = rebuild_hunk(&d.hunks[0], 0, &[sel(0, None, Some(2))], false).unwrap();
    // old_count = a,c = 2 ; new_count = a,b,c = 3
    assert!(out.starts_with("@@ -1,2 +1,3 @@\n"), "header: {}", out);
    assert!(out.contains("+b"));
  }

  #[test]
  fn no_newline_marker_preserved() {
    let text = "--- a/f\n+++ b/f\n@@ -1 +1 @@\n-a\n+b\n\\ No newline at end of file\n";
    let d = parse_diff(text).unwrap();
    let out = rebuild_hunk(&d.hunks[0], 0, &[sel(0, Some(1), None), sel(0, None, Some(1))], false).unwrap();
    assert!(out.contains("\\ No newline at end of file"));
  }

  // ----- Round-trip tests: build a patch and let real `git apply` validate it. -----

  use std::path::{Path, PathBuf};
  use std::process::Command;

  fn git(dir: &Path, args: &[&str]) -> std::process::Output {
    Command::new("git").arg("-C").arg(dir).args(args).output().expect("git run")
  }

  /// Create a throwaway repo with one committed file, return its dir.
  fn scratch_repo(name: &str, initial: &str) -> Option<PathBuf> {
    if super::run_git(None, &["--version"]).is_err() {
      return None; // git unavailable; skip
    }
    let mut dir = std::env::temp_dir();
    dir.push(format!("gitwyrm_patch_test_{name}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    git(&dir, &["init", "-q"]);
    git(&dir, &["config", "user.email", "t@t.t"]);
    git(&dir, &["config", "user.name", "t"]);
    git(&dir, &["config", "core.autocrlf", "false"]);
    std::fs::write(dir.join("f.txt"), initial).unwrap();
    git(&dir, &["add", "f.txt"]);
    git(&dir, &["commit", "-qm", "init"]);
    Some(dir)
  }

  fn staged_content(dir: &Path) -> String {
    let out = git(dir, &["show", ":f.txt"]);
    String::from_utf8_lossy(&out.stdout).into_owned()
  }

  /// A single change surrounded by context (line N modified in an otherwise
  /// long file) produces two distant hunks when two such changes are far apart.
  #[test]
  fn roundtrip_stage_one_of_two_hunks() {
    // 20 lines; change line 2 and line 15 -> two hunks (context does not overlap).
    let base = (1..=20).map(|n| format!("l{n}\n")).collect::<String>();
    let Some(dir) = scratch_repo("two_hunks", &base) else { return };
    let dpath = dir.to_string_lossy().into_owned();
    let mut modified: Vec<String> = (1..=20).map(|n| format!("l{n}\n")).collect();
    modified[1] = "L2\n".into();
    modified[14] = "L15\n".into();
    std::fs::write(dir.join("f.txt"), modified.concat()).unwrap();

    let dargs = diff_args_for(PatchTarget::Unstaged, "f.txt");
    let dargs_ref: Vec<&str> = dargs.iter().map(String::as_str).collect();
    let raw = super::run_git(Some(&dpath), &dargs_ref).unwrap().stdout;
    let parsed = parse_diff(&raw).unwrap();
    assert_eq!(parsed.hunks.len(), 2, "expected two hunks, diff:\n{raw}");

    // Stage only hunk 0: -l2 (old 2), +L2 (new 2).
    let selection = vec![sel(0, Some(2), None), sel(0, None, Some(2))];
    let patch = build_patch(&dpath, "f.txt", &dargs_ref, &selection, false).unwrap();
    super::run_git_stdin(Some(&dpath), &["apply", "--cached", "-"], patch.as_bytes()).unwrap();

    // Index has L2 but still l15.
    let mut expected: Vec<String> = (1..=20).map(|n| format!("l{n}\n")).collect();
    expected[1] = "L2\n".into();
    assert_eq!(staged_content(&dir), expected.concat(), "diff was:\n{raw}");
    let _ = std::fs::remove_dir_all(&dir);
  }

  /// Whole-hunk staging of a contiguous replace block: select every changed
  /// line in the hunk. This is the well-defined case git itself supports.
  #[test]
  fn roundtrip_stage_whole_replace_block() {
    let Some(dir) = scratch_repo("block", "one\ntwo\nthree\nfour\n") else { return };
    let dpath = dir.to_string_lossy().into_owned();
    std::fs::write(dir.join("f.txt"), "one\nTWO\nTHREE\nfour\n").unwrap();

    let dargs = diff_args_for(PatchTarget::Unstaged, "f.txt");
    let dargs_ref: Vec<&str> = dargs.iter().map(String::as_str).collect();
    // Every changed line: -two(2) -three(3) +TWO(2) +THREE(3).
    let selection = vec![
      sel(0, Some(2), None),
      sel(0, Some(3), None),
      sel(0, None, Some(2)),
      sel(0, None, Some(3)),
    ];
    let patch = build_patch(&dpath, "f.txt", &dargs_ref, &selection, false).unwrap();
    super::run_git_stdin(Some(&dpath), &["apply", "--cached", "-"], patch.as_bytes()).unwrap();
    assert_eq!(staged_content(&dir), "one\nTWO\nTHREE\nfour\n");
    let _ = std::fs::remove_dir_all(&dir);
  }

  /// Line-level staging is well-defined when a context line separates the two
  /// changes (git offers "split" here too). Stage only the first change.
  #[test]
  fn roundtrip_stage_single_separated_line() {
    let Some(dir) = scratch_repo("sep", "one\ntwo\nMID\nthree\nfour\n") else { return };
    let dpath = dir.to_string_lossy().into_owned();
    // Change "two"->"TWO" and "three"->"THREE", separated by context "MID".
    std::fs::write(dir.join("f.txt"), "one\nTWO\nMID\nTHREE\nfour\n").unwrap();

    let dargs = diff_args_for(PatchTarget::Unstaged, "f.txt");
    let dargs_ref: Vec<&str> = dargs.iter().map(String::as_str).collect();
    // Stage only two->TWO: -two(old 2), +TWO(new 2).
    let selection = vec![sel(0, Some(2), None), sel(0, None, Some(2))];
    let patch = build_patch(&dpath, "f.txt", &dargs_ref, &selection, false).unwrap();
    super::run_git_stdin(Some(&dpath), &["apply", "--cached", "-"], patch.as_bytes()).unwrap();
    // three stays unchanged in the index.
    assert_eq!(staged_content(&dir), "one\nTWO\nMID\nthree\nfour\n");
    let _ = std::fs::remove_dir_all(&dir);
  }

  #[test]
  fn roundtrip_unstage_reverse_applies() {
    // Two changes separated by context so line selection is well-defined.
    let Some(dir) = scratch_repo("unstage", "l1\nl2\nMID\nl3\nl4\n") else { return };
    let dpath = dir.to_string_lossy().into_owned();
    std::fs::write(dir.join("f.txt"), "l1\nL2\nMID\nl3\nL4\n").unwrap();
    git(&dir, &["add", "f.txt"]); // stage everything

    let dargs = diff_args_for(PatchTarget::Staged, "f.txt");
    let dargs_ref: Vec<&str> = dargs.iter().map(String::as_str).collect();
    // Unstage only the L2 change: -l2(old 2), +L2(new 2).
    let selection = vec![sel(0, Some(2), None), sel(0, None, Some(2))];
    let patch = build_patch(&dpath, "f.txt", &dargs_ref, &selection, true).unwrap();
    super::run_git_stdin(Some(&dpath), &["apply", "--cached", "--reverse", "-"], patch.as_bytes())
      .unwrap();

    // Index: l2 restored, L4 kept.
    assert_eq!(staged_content(&dir), "l1\nl2\nMID\nl3\nL4\n");
    let _ = std::fs::remove_dir_all(&dir);
  }

  /// Slidable insertion (duplicate/blank-separated block) is where git-CLI's
  /// indent heuristic diverges from libgit2. With the heuristic pinned off, the
  /// CLI diff line numbers must match libgit2's so a single-line selection
  /// stages the intended line. We assert the staged result is exactly the one
  /// selected line, which only holds if anchoring agrees.
  #[test]
  fn roundtrip_slidable_block_single_line() {
    // A file whose insertion point is ambiguous (repeated blank lines) so the
    // indent heuristic would anchor it differently than libgit2 if not pinned.
    // Insert exactly ONE line so the single-line selection is a valid, isolated
    // change (a contiguous multi-line insert would require whole-block staging).
    let base = "alpha\n\nbeta\n\ngamma\n";
    let Some(dir) = scratch_repo("slidable", base) else { return };
    let dpath = dir.to_string_lossy().into_owned();
    std::fs::write(dir.join("f.txt"), "alpha\n\nbeta\ninserted\n\ngamma\n").unwrap();

    let dargs = diff_args_for(PatchTarget::Unstaged, "f.txt");
    let dargs_ref: Vec<&str> = dargs.iter().map(String::as_str).collect();
    let raw = super::run_git(Some(&dpath), &dargs_ref).unwrap().stdout;
    let parsed = parse_diff(&raw).unwrap();
    // Grab the first added line's new_no from the (pinned) CLI diff itself, so
    // this test verifies the anchoring the code actually uses.
    let mut cur_new = parsed.hunks[0].new_start;
    let mut first_added: Option<u32> = None;
    for l in &parsed.hunks[0].lines {
      match l.as_bytes().first() {
        Some(b'+') => {
          if first_added.is_none() {
            first_added = Some(cur_new);
          }
          cur_new += 1;
        }
        Some(b'-') => {}
        _ => cur_new += 1,
      }
    }
    let n = first_added.expect("an added line");
    let selection = vec![sel(0, None, Some(n))];
    let patch = build_patch(&dpath, "f.txt", &dargs_ref, &selection, false).unwrap();
    // The rebuilt single-line patch must apply cleanly (context agrees).
    super::run_git_stdin(Some(&dpath), &["apply", "--cached", "-"], patch.as_bytes()).unwrap();
    // Exactly one line entered the index vs HEAD.
    let staged_diff = super::run_git(
      Some(&dpath),
      &["diff", "--cached", "--numstat", "--", "f.txt"],
    )
    .unwrap()
    .stdout;
    // numstat: "<added>\t<deleted>\tpath"
    let added: u32 = staged_diff.split('\t').next().unwrap_or("0").trim().parse().unwrap_or(99);
    assert_eq!(added, 1, "expected exactly one added line staged, numstat: {staged_diff}");
    let _ = std::fs::remove_dir_all(&dir);
  }

  #[test]
  fn partial_contiguous_block_rejected() {
    // -two -three +TWO +THREE is one contiguous run; selecting only two->TWO
    // (part of it) must be rejected by the backstop.
    let d = parse_diff(SAMPLE).unwrap();
    let selection = vec![sel(0, Some(2), None), sel(0, None, Some(2))];
    let err = validate_contiguous_blocks(&d.hunks[0], 0, &selection);
    assert!(err.is_err(), "partial contiguous selection should be rejected");

    // Selecting the WHOLE block is allowed.
    let whole = vec![
      sel(0, Some(2), None),
      sel(0, Some(3), None),
      sel(0, None, Some(2)),
      sel(0, None, Some(3)),
    ];
    assert!(validate_contiguous_blocks(&d.hunks[0], 0, &whole).is_ok());
  }

  #[test]
  fn multi_file_diff_rejected() {
    // parse-level guard: a two-file raw diff must be refused by build_patch.
    // We simulate by calling build_patch against a repo with two changed files
    // but a pathspec that (deliberately) matches both via a dir.
    let Some(dir) = scratch_repo("multi", "x\n") else { return };
    let dpath = dir.to_string_lossy().into_owned();
    // Commit a second file so both are tracked, then modify both (unstaged).
    std::fs::write(dir.join("g.txt"), "y\n").unwrap();
    git(&dir, &["add", "g.txt"]);
    git(&dir, &["commit", "-qm", "add g"]);
    std::fs::write(dir.join("f.txt"), "X\n").unwrap();
    std::fs::write(dir.join("g.txt"), "Y\n").unwrap();
    // Diff the whole tree (".") -> two files.
    let dargs: Vec<String> = vec![
      "diff".into(),
      "--no-color".into(),
      "-U3".into(),
      "--".into(),
      ".".into(),
    ];
    let dargs_ref: Vec<&str> = dargs.iter().map(String::as_str).collect();
    let err = build_patch(&dpath, ".", &dargs_ref, &[sel(0, None, Some(1))], false);
    assert!(err.is_err(), "multi-file diff should be rejected");
    let _ = std::fs::remove_dir_all(&dir);
  }

  #[test]
  fn roundtrip_discard_single_line_from_workdir() {
    let Some(dir) = scratch_repo("discard", "l1\nl2\nMID\nl3\nl4\n") else { return };
    let dpath = dir.to_string_lossy().into_owned();
    // Two unstaged changes separated by context.
    std::fs::write(dir.join("f.txt"), "l1\nL2\nMID\nl3\nL4\n").unwrap();

    let dargs = diff_args_for(PatchTarget::Unstaged, "f.txt");
    let dargs_ref: Vec<&str> = dargs.iter().map(String::as_str).collect();
    // Discard only L2 (reverse the unstaged diff against the workdir).
    let selection = vec![sel(0, Some(2), None), sel(0, None, Some(2))];
    let patch = build_patch(&dpath, "f.txt", &dargs_ref, &selection, true).unwrap();
    super::run_git_stdin(Some(&dpath), &["apply", "--reverse", "-"], patch.as_bytes()).unwrap();

    // Workdir: L2 reverted to l2, L4 kept.
    let workdir = std::fs::read_to_string(dir.join("f.txt")).unwrap();
    assert_eq!(workdir, "l1\nl2\nMID\nl3\nL4\n", "workdir:\n{workdir}");
    let _ = std::fs::remove_dir_all(&dir);
  }
}
