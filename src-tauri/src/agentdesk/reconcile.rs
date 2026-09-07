//! Project path reconciliation (task 2.4): map an external client's recorded
//! project path onto a known GitWyrm repository, without ever hiding a path
//! that could not be matched. Spec: "keep unresolved paths visible."

use serde::{Deserialize, Serialize};
use specta::Type;

/// One entry a reconciliation candidate list is built from -- deliberately
/// just `id`/`name`/`path`, matching [`crate::settings::RecentRepo`]'s shape
/// plus the repo ID a session header needs, so this module has no dependency
/// on `settings` or any Tauri type and can be unit tested directly.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KnownRepo {
    pub repo_id: String,
    pub repo_name: String,
    pub repo_path: String,
}

/// The result of trying to match one external session's recorded project
/// path against known repositories.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ProjectResolution {
    /// Matched exactly one known repo.
    #[serde(rename_all = "camelCase")]
    Resolved {
        repo_id: String,
        repo_name: String,
        repo_path: String,
    },
    /// The external client recorded no project path at all for this
    /// session -- distinct from `Unresolved`, since there is nothing to
    /// retry a match against later even if the user adds the repo.
    NoProjectRecorded,
    /// A path was recorded but does not match any known repo. Carries the
    /// raw path back so the UI can show *what* could not be found and offer
    /// "Open this folder" / "Add as repo" rather than silently dropping the
    /// session (spec: "keep unresolved paths visible").
    #[serde(rename_all = "camelCase")]
    Unresolved { recorded_path: String },
}

/// Normalize a path for comparison: forward slashes only, no trailing slash,
/// and a lowercased Windows drive letter. Not a general path canonicalizer --
/// this never touches the filesystem (a foreign session's project may not even
/// exist on this machine), so it is pure string normalization only.
///
/// The drive letter is folded and nothing else is. That split is the whole
/// point, so it is worth being exact about why.
///
/// `C:` and `c:` name the same volume on every Windows machine, whatever the
/// filesystem underneath -- it is a property of the path syntax, not of how a
/// disk was formatted. And the fold is load-bearing: VS Code records its
/// workspace as `file:///c%3A/code/foo`, which decodes to a *lowercase* drive
/// letter (see `adapters::vscode_copilot::file_uri_to_path`), while GitWyrm's
/// own repo list stores `C:/code/foo`. Without this, every Copilot import on
/// Windows would fail to find its project.
///
/// The rest of the path is compared exactly. This used to lowercase the whole
/// string, on the reasoning that "Windows paths are case-insensitive" -- but
/// GitWyrm supports Linux (PRODUCT.md), where `/home/dev/Repo` and
/// `/home/dev/repo` are two different directories. Folding them together
/// attached an imported chat to the wrong project silently, which is exactly
/// what `resolve_project_path` says below is worse than not matching at all.
///
/// Deciding by build target instead (`cfg(windows)`) would only move the
/// guess: a Windows directory can be made case-sensitive, macOS can be
/// formatted either way, and the recorded path may belong to a machine this
/// build never runs on. The drive letter is the one part that needs no guess.
fn normalize(path: &str) -> String {
    let forward = path.replace('\\', "/");
    let trimmed = forward.trim_end_matches('/');
    fold_drive_letter(trimmed)
}

/// Lowercases a leading `C:` when the path starts with a Windows drive
/// letter, and changes nothing otherwise.
fn fold_drive_letter(path: &str) -> String {
    let bytes = path.as_bytes();
    let is_drive = bytes.len() >= 2
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && (bytes.len() == 2 || bytes[2] == b'/');
    if !is_drive {
        return path.to_string();
    }
    let mut out = String::with_capacity(path.len());
    out.push(bytes[0].to_ascii_lowercase() as char);
    out.push_str(&path[1..]);
    out
}

/// Reconcile one external `project_path` (as returned by an adapter's
/// [`super::adapters::ExternalSessionSummary::project_path`]) against a list
/// of known repos. Exact match after normalization only -- no fuzzy/prefix
/// matching, since a wrong guess (matching a session to the wrong repo)
/// is worse than an honest `Unresolved`.
pub fn resolve_project_path(
    project_path: Option<&str>,
    known_repos: &[KnownRepo],
) -> ProjectResolution {
    let Some(path) = project_path else {
        return ProjectResolution::NoProjectRecorded;
    };
    if path.trim().is_empty() {
        return ProjectResolution::NoProjectRecorded;
    }

    let normalized = normalize(path);
    known_repos
        .iter()
        .find(|r| normalize(&r.repo_path) == normalized)
        .map(|r| ProjectResolution::Resolved {
            repo_id: r.repo_id.clone(),
            repo_name: r.repo_name.clone(),
            repo_path: r.repo_path.clone(),
        })
        .unwrap_or(ProjectResolution::Unresolved {
            recorded_path: path.to_string(),
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repos() -> Vec<KnownRepo> {
        vec![
            KnownRepo {
                repo_id: "repo-1".into(),
                repo_name: "GitWyrm".into(),
                repo_path: "C:/code/GitWyrm".into(),
            },
            KnownRepo {
                repo_id: "repo-2".into(),
                repo_name: "widgets".into(),
                repo_path: "C:/code/widgets".into(),
            },
        ]
    }

    #[test]
    fn no_project_path_recorded_is_its_own_variant() {
        assert_eq!(
            resolve_project_path(None, &repos()),
            ProjectResolution::NoProjectRecorded
        );
        assert_eq!(
            resolve_project_path(Some(""), &repos()),
            ProjectResolution::NoProjectRecorded
        );
        assert_eq!(
            resolve_project_path(Some("   "), &repos()),
            ProjectResolution::NoProjectRecorded
        );
    }

    #[test]
    fn an_exact_match_resolves() {
        let result = resolve_project_path(Some("C:/code/GitWyrm"), &repos());
        assert_eq!(
            result,
            ProjectResolution::Resolved {
                repo_id: "repo-1".into(),
                repo_name: "GitWyrm".into(),
                repo_path: "C:/code/GitWyrm".into(),
            }
        );
    }

    /// Separators and drive-letter case are normalized; folder names are not.
    ///
    /// This test used to assert that `c:\Code\GITWYRM` matched
    /// `C:/code/GitWyrm` -- every part case-folded. It was the only evidence
    /// for folding whole paths, and it was an invented input: no adapter
    /// produces a segment whose case differs from the real folder. Meanwhile
    /// the folding it justified silently attached imported chats to the wrong
    /// project on Linux, where two folders really can differ only by case.
    #[test]
    fn separators_and_drive_case_are_normalized_but_folder_names_are_not() {
        // Backslashes and a lowercase drive letter still find the repo. This
        // is the shape VS Code actually records.
        let matched = resolve_project_path(Some(r"c:\code\GitWyrm"), &repos());
        assert!(
            matches!(matched, ProjectResolution::Resolved { .. }),
            "separators and drive-letter case must not prevent a match"
        );

        // A folder spelled differently is a different folder, and saying so is
        // the point: `resolve_project_path` prefers an honest miss to a wrong
        // guess, and the miss is shown with the path and an offer to link it.
        let missed = resolve_project_path(Some("C:/code/GITWYRM"), &repos());
        assert!(
            matches!(missed, ProjectResolution::Unresolved { .. }),
            "a folder name that differs by case must not be claimed as a match"
        );
    }

    /// Only a real drive letter folds. Anything that merely looks like one
    /// keeps its case, so a Linux folder called `ABC:` or a relative
    /// `C:relative` path is never quietly rewritten.
    #[test]
    fn only_a_real_drive_letter_is_folded() {
        assert_eq!(fold_drive_letter("C:/code/x"), "c:/code/x");
        assert_eq!(fold_drive_letter("c:/code/x"), "c:/code/x");
        assert_eq!(fold_drive_letter("Z:"), "z:");
        // Not drive letters: no separator after the colon, more than one
        // letter before it, or no colon at all.
        assert_eq!(fold_drive_letter("C:relative/x"), "C:relative/x");
        assert_eq!(fold_drive_letter("ABC:/x"), "ABC:/x");
        assert_eq!(fold_drive_letter("/home/dev/Repo"), "/home/dev/Repo");
        assert_eq!(fold_drive_letter(""), "");
    }

    /// The case that made whole-path folding wrong.
    ///
    /// On Linux these are two different directories. Folded together, a chat
    /// recorded against one was silently attached to the other -- the wrong
    /// guess this module's own doc comment says is worse than no match.
    #[test]
    fn two_linux_folders_differing_only_by_case_are_not_the_same_project() {
        let repos = vec![KnownRepo {
            repo_id: "repo-9".into(),
            repo_name: "repo".into(),
            repo_path: "/home/dev/repo".into(),
        }];
        let result = resolve_project_path(Some("/home/dev/Repo"), &repos);
        assert!(
            matches!(result, ProjectResolution::Unresolved { .. }),
            "/home/dev/Repo and /home/dev/repo are different folders"
        );
    }

    /// A drive letter is folded wherever it appears, in either direction.
    #[test]
    fn a_drive_letter_matches_in_either_case() {
        let repos = vec![KnownRepo {
            repo_id: "repo-8".into(),
            repo_name: "lower".into(),
            repo_path: "c:/code/thing".into(),
        }];
        assert!(matches!(
            resolve_project_path(Some("C:/code/thing"), &repos),
            ProjectResolution::Resolved { .. }
        ));
    }

    #[test]
    fn a_trailing_slash_does_not_prevent_a_match() {
        let result = resolve_project_path(Some("C:/code/GitWyrm/"), &repos());
        assert!(matches!(result, ProjectResolution::Resolved { .. }));
    }

    #[test]
    fn an_unmatched_path_is_unresolved_and_keeps_the_original_text() {
        let result = resolve_project_path(Some("C:/code/some-other-project"), &repos());
        assert_eq!(
            result,
            ProjectResolution::Unresolved {
                recorded_path: "C:/code/some-other-project".into(),
            }
        );
    }

    #[test]
    fn a_moved_project_no_longer_matches_and_stays_visibly_unresolved() {
        // Simulates task 3.6's "moved project" fixture case: the path the
        // external client recorded still exists in its own history, but the
        // repo is no longer at that location, so it should not silently
        // disappear -- Unresolved with the recorded path intact is exactly
        // that "still visible" outcome.
        let stale_repos = vec![KnownRepo {
            repo_id: "repo-1".into(),
            repo_name: "GitWyrm".into(),
            repo_path: "D:/new-location/GitWyrm".into(),
        }];
        let result = resolve_project_path(Some("C:/code/GitWyrm"), &stale_repos);
        assert_eq!(
            result,
            ProjectResolution::Unresolved {
                recorded_path: "C:/code/GitWyrm".into(),
            }
        );
    }
}
