//! Where a change has been: the commits that touched its folder.
//!
//! The mockup shows a History tab with attributed entries ("drafted with
//! Copilot · reviewed by you"). Attribution by trailer belongs to
//! `add-spec-commit-links`, so this derives what git already knows: every commit
//! that touched `openspec/changes/<id>/`, newest first, with the author and a
//! plain-language summary. That is real history rather than a placeholder, and
//! the trailer work later enriches the same rows.

use std::path::Path;

use serde::{Deserialize, Serialize};
use specta::Type;

/// One thing that happened to a change.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct SpecHistoryEntry {
  /// Short sha, or empty for entries that are not commits.
  pub short_sha: String,
  /// Commit subject, as written.
  pub summary: String,
  /// Who made it.
  pub author: String,
  /// Unix epoch seconds.
  pub time: f64,
  /// True when the commit message carries an `Assisted-by:` trailer, so the UI
  /// can mark AI-assisted work without guessing.
  pub ai_assisted: bool,
}

/// Commits that touched a change's folder, newest first, capped at `limit`.
///
/// Shells out rather than using git2: `--follow`-less pathspec log with a custom
/// format is a one-liner for the CLI and a walk-plus-diff for git2, and this runs
/// once per change view.
pub fn change_history(
  repo_root: &Path,
  change_id: &str,
  limit: u32,
) -> Result<Vec<SpecHistoryEntry>, crate::error::AppError> {
  let pathspec = format!("openspec/changes/{change_id}/");
  let max = format!("-n{limit}");
  // Unit separator between fields, record separator between commits: commit
  // subjects contain almost anything, so a printable delimiter is not safe.
  //
  // Written as git's own `%x1f`/`%x1e` escapes rather than literal control bytes
  // in the argument. Passing raw 0x1e through the Windows command line killed the
  // backend process outright -- git printed the right thing when tested in a
  // shell, but the same bytes in a spawned argument did not survive.
  let out = crate::git::shell::run_git(
    Some(&repo_root.to_string_lossy()),
    &[
      "log",
      &max,
      "--format=%h%x1f%s%x1f%an%x1f%at%x1f%(trailers:key=Assisted-by,valueonly)%x1e",
      "--",
      &pathspec,
    ],
  )?;

  Ok(parse_history(&out.stdout))
}

/// Splits the `git log` output produced above into entries. Kept separate from
/// the shell call so it can be tested without a repository.
fn parse_history(stdout: &str) -> Vec<SpecHistoryEntry> {
  stdout
    .split('\x1e')
    .map(str::trim)
    .filter(|record| !record.is_empty())
    .filter_map(|record| {
      let mut fields = record.split('\x1f');
      let short_sha = fields.next()?.trim().to_string();
      let summary = fields.next()?.trim().to_string();
      let author = fields.next()?.trim().to_string();
      let time: f64 = fields.next()?.trim().parse().ok()?;
      // Absent trailer yields an empty field, which is the common case.
      let ai_assisted = fields.next().map(|t| !t.trim().is_empty()).unwrap_or(false);
      Some(SpecHistoryEntry { short_sha, summary, author, time, ai_assisted })
    })
    .collect()
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn parses_log_records() {
    let stdout = "a1b2c3\x1fnew: Add the thing\x1fJeremy\x1f1785000000\x1f\x1e\n\
                  d4e5f6\x1fimproved: Tidy it up\x1fJeremy\x1f1784900000\x1fCopilot\x1e\n";
    let entries = parse_history(stdout);
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].short_sha, "a1b2c3");
    assert_eq!(entries[0].summary, "new: Add the thing");
    assert_eq!(entries[0].author, "Jeremy");
    assert_eq!(entries[0].time, 1785000000.0);
    assert!(!entries[0].ai_assisted, "no trailer means not AI-assisted");
    assert!(entries[1].ai_assisted, "Assisted-by trailer marks the entry");
  }

  #[test]
  fn tolerates_subjects_containing_delimiters_and_blank_output() {
    // A subject with a colon, quotes, and unicode must survive intact.
    let stdout = "aaa\x1ffixes: \"quoted\" - em dash test\x1fA B\x1f1\x1f\x1e";
    let entries = parse_history(stdout);
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].summary, "fixes: \"quoted\" - em dash test");

    // A change never committed has no history, which is not an error.
    assert!(parse_history("").is_empty());
    assert!(parse_history("\n\x1e\n").is_empty());
  }

  #[test]
  fn skips_malformed_records_without_dropping_good_ones() {
    let stdout = "short-record-no-fields\x1e\
                  bbb\x1fgood one\x1fAuthor\x1f1700000000\x1f\x1e";
    let entries = parse_history(stdout);
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].summary, "good one");
  }
}
