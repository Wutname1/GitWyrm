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
    // One field per line, with an explicit end-of-record marker.
    //
    // A commit subject cannot contain a newline, so line-per-field is unambiguous
    // without control-byte separators or `%(trailers:...)` -- both of which are
    // portability risks across the git versions GitWyrm may resolve (system git,
    // or the MinGit copy it bundles). The Assisted-by check reads the body, which
    // needs no format extension at all.
    let out = crate::git::shell::run_git(
        Some(&repo_root.to_string_lossy()),
        &[
            "log",
            &max,
            "--format=%h%n%s%n%an%n%at%n%b%n@@RECORD@@",
            "--",
            &pathspec,
        ],
    )?;

    Ok(parse_history(&out.stdout))
}

/// Splits the `git log` output produced above into entries. Kept separate from
/// the shell call so it can be tested without a repository.
///
/// Layout per record: sha, subject, author, unix time, then the body (any number
/// of lines) up to the `@@RECORD@@` marker.
fn parse_history(stdout: &str) -> Vec<SpecHistoryEntry> {
    stdout
        .split("@@RECORD@@")
        .filter(|record| !record.trim().is_empty())
        .filter_map(|record| {
            let mut lines = record.trim_start_matches(['\r', '\n']).lines();
            let short_sha = lines.next()?.trim().to_string();
            let summary = lines.next()?.trim().to_string();
            let author = lines.next()?.trim().to_string();
            let time: f64 = lines.next()?.trim().parse().ok()?;
            // A commit is AI-assisted when its body carries the trailer. Read it here
            // rather than asking git to extract it -- see the format note above.
            let ai_assisted = lines.any(|line| {
                line.trim_start()
                    .to_ascii_lowercase()
                    .starts_with("assisted-by:")
            });
            if short_sha.is_empty() {
                return None;
            }
            Some(SpecHistoryEntry {
                short_sha,
                summary,
                author,
                time,
                ai_assisted,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_log_records() {
        let stdout = "a1b2c3\nnew: Add the thing\nJeremy\n1785000000\n\n@@RECORD@@\n\
                  d4e5f6\nimproved: Tidy it up\nJeremy\n1784900000\nBody line\nAssisted-by: Copilot\n\n@@RECORD@@\n";
        let entries = parse_history(stdout);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].short_sha, "a1b2c3");
        assert_eq!(entries[0].summary, "new: Add the thing");
        assert_eq!(entries[0].author, "Jeremy");
        assert_eq!(entries[0].time, 1785000000.0);
        assert!(!entries[0].ai_assisted, "no trailer means not AI-assisted");
        assert!(
            entries[1].ai_assisted,
            "Assisted-by trailer marks the entry"
        );
    }

    #[test]
    fn tolerates_awkward_subjects_and_blank_output() {
        // Colons, quotes, and a multi-line body must not confuse the fields.
        let stdout = "aaa\nfixes: \"quoted\" - dash test\nA B\n1\nsome body\nmore body\n@@RECORD@@";
        let entries = parse_history(stdout);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].summary, "fixes: \"quoted\" - dash test");
        assert_eq!(entries[0].author, "A B");

        // A change never committed has no history, which is not an error.
        assert!(parse_history("").is_empty());
        assert!(parse_history("\n@@RECORD@@\n").is_empty());
        assert!(parse_history("   \n  \n").is_empty());
    }

    #[test]
    fn skips_malformed_records_without_dropping_good_ones() {
        let stdout = "only-one-line@@RECORD@@\
                  bbb\ngood one\nAuthor\n1700000000\n\n@@RECORD@@";
        let entries = parse_history(stdout);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].summary, "good one");
    }

    #[test]
    fn ignores_a_trailer_like_line_in_the_subject() {
        // The subject is field 2, never scanned for trailers, so a commit that
        // merely mentions the trailer is not marked as AI-assisted.
        let stdout = "ccc\nfixes: drop the Assisted-by: trailer\nA\n1\n\n@@RECORD@@";
        let entries = parse_history(stdout);
        assert_eq!(entries.len(), 1);
        assert!(!entries[0].ai_assisted);
    }
}
