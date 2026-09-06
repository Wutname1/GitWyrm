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

/// Normalize a path for comparison: lowercase (Windows paths are
/// case-insensitive and every fixture/real sample seen mixes drive-letter
/// case), forward slashes only, and no trailing slash. Not a general path
/// canonicalizer -- this never touches the filesystem (a foreign session's
/// project may not even exist on this machine), so it is pure string
/// normalization only.
fn normalize(path: &str) -> String {
    let forward = path.replace('\\', "/");
    let trimmed = forward.trim_end_matches('/');
    trimmed.to_lowercase()
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

    #[test]
    fn backslash_paths_and_case_still_match() {
        let result = resolve_project_path(Some(r"c:\Code\GITWYRM"), &repos());
        assert!(matches!(result, ProjectResolution::Resolved { .. }));
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

#[cfg(test)]
mod audit_probe2 {
    use super::*;
    #[test]
    fn probe_percent_encoded_path_fails_to_resolve() {
        let repos = vec![KnownRepo{repo_id:"1".into(), repo_name:"my project".into(), repo_path:"C:/code/my project".into()}];
        println!("{:?}", resolve_project_path(Some("c:/code/my%20project"), &repos));
        println!("posix: {:?}", resolve_project_path(Some("home/me/x"), &vec![KnownRepo{repo_id:"2".into(),repo_name:"x".into(),repo_path:"/home/me/x".into()}]));
    }
}
