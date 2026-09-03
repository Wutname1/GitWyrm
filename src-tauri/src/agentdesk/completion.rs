//! Deciding whether a helper actually did what it was asked to do.
//!
//! A proposed helper carries a [`CompletionCondition`]: report a result, make
//! a named check pass, or touch a named set of files. Two of those three were
//! modelled and never evaluated, so a lead could ask for "the tests pass" and
//! get a helper that stopped early and still counted as finished. The only
//! condition that ever meant anything was the weakest one.
//!
//! This module answers the question from evidence already on the session:
//! the checks the helper ran (recorded on its own transcript rows) and the
//! files its result records as changed. Nothing is re-run here. Re-running a
//! check to grade a helper would spend the person's machine on work the
//! helper already did, and would be answering a different question: whether
//! it passes NOW, rather than whether the helper made it pass.

use crate::agentdesk::graph::CompletionCondition;
use crate::agentdesk::result::{CheckRunOutcome, ResultCheckOutcome, ResultRecord};

/// Whether a helper met its condition, and what to say when it did not.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompletionVerdict {
    /// The condition was met, or there was none to meet.
    Met,
    /// The helper stopped without meeting it. `reason` is the sentence shown
    /// in the chat, written for someone who did not read the transcript.
    Unmet { reason: String },
}

impl CompletionVerdict {
    pub fn is_met(&self) -> bool {
        matches!(self, CompletionVerdict::Met)
    }
}

/// Judges one helper against its own completion condition.
///
/// `checks` are the checks that helper ran; `result` is its result record, if
/// one was built. Both are what the run itself produced -- this never asks
/// the model whether it thinks it succeeded, because a helper's own account
/// is exactly what a condition exists to verify.
pub fn judge(
    condition: Option<&CompletionCondition>,
    checks: &[ResultCheckOutcome],
    result: Option<&ResultRecord>,
) -> CompletionVerdict {
    let Some(condition) = condition else {
        // No condition recorded: an older session, or the lead itself.
        // Finishing is finishing.
        return CompletionVerdict::Met;
    };

    match condition {
        // The helper finishing IS the report. This is the condition every
        // proposal uses today, and it is deliberately the weakest one.
        CompletionCondition::ReportsResult => CompletionVerdict::Met,

        CompletionCondition::ChecksPass { command } => {
            let matching: Vec<&ResultCheckOutcome> = checks
                .iter()
                .filter(|c| check_matches(&c.command_name, command))
                .collect();
            if matching.is_empty() {
                return CompletionVerdict::Unmet {
                    reason: format!(
                        "It was asked to make \"{command}\" pass, but it never ran that check."
                    ),
                };
            }
            // The last run of a check is the one that counts: a helper that
            // fixed a failure and re-ran it has made it pass.
            let last = matching[matching.len() - 1];
            match last.outcome {
                CheckRunOutcome::Passed => CompletionVerdict::Met,
                CheckRunOutcome::Failed => CompletionVerdict::Unmet {
                    reason: format!("It was asked to make \"{command}\" pass, and it is still failing."),
                },
                // Anything that is not a clean pass is not a pass. A check
                // that could not run tells us nothing about the work.
                _ => CompletionVerdict::Unmet {
                    reason: format!(
                        "It was asked to make \"{command}\" pass, but that check did not finish."
                    ),
                },
            }
        }

        CompletionCondition::FilesChanged { paths } => {
            let Some(result) = result else {
                return CompletionVerdict::Unmet {
                    reason: "It was asked to change specific files, but it left no changes at all."
                        .to_string(),
                };
            };
            let changed: Vec<&str> = result
                .changed_paths
                .iter()
                .map(|p| p.path.as_str())
                .collect();
            let missing: Vec<&String> = paths
                .iter()
                .filter(|wanted| !changed.iter().any(|actual| path_matches(actual, wanted)))
                .collect();
            if missing.is_empty() {
                return CompletionVerdict::Met;
            }
            let named = missing
                .iter()
                .map(|p| p.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            CompletionVerdict::Unmet {
                reason: if missing.len() == 1 {
                    format!("It was asked to change {named}, and did not.")
                } else {
                    format!("It was asked to change these and did not: {named}.")
                },
            }
        }
    }
}

/// Whether a recorded check is the one the condition names.
///
/// Compared loosely on purpose: a condition says `cargo test` while the
/// recorded name may be `cargo test --lib` or `Tests (cargo test)`. Demanding
/// an exact string would fail a helper that ran precisely what was asked.
fn check_matches(recorded: &str, wanted: &str) -> bool {
    let recorded = recorded.trim().to_lowercase();
    let wanted = wanted.trim().to_lowercase();
    recorded == wanted || recorded.contains(&wanted) || wanted.contains(&recorded)
}

/// Whether a changed path satisfies a wanted path.
///
/// A condition may name a file (`src/parser.rs`), a folder (`src/`), or a
/// simple glob (`src/**`). Anything more elaborate than a trailing wildcard
/// is compared as a plain prefix, which errs toward accepting work that was
/// genuinely done rather than failing a helper on pattern syntax.
fn path_matches(changed: &str, wanted: &str) -> bool {
    let changed = normalize(changed);
    let wanted = normalize(wanted);
    if changed == wanted {
        return true;
    }
    let base = wanted
        .trim_end_matches("**")
        .trim_end_matches('*')
        .trim_end_matches('/');
    if base.is_empty() {
        // A bare `*` or `**` means "anything", which any change satisfies.
        return true;
    }
    changed == base || changed.starts_with(&format!("{base}/"))
}

fn normalize(path: &str) -> String {
    path.replace('\\', "/").trim_start_matches("./").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agentdesk::result::{ResultChangedPath, ResultOutcomeKind, ResultState};

    fn check(name: &str, outcome: CheckRunOutcome) -> ResultCheckOutcome {
        ResultCheckOutcome {
            command_name: name.to_string(),
            outcome,
            summary: None,
        }
    }

    fn result_with(paths: &[&str]) -> ResultRecord {
        ResultRecord {
            execution_id: "h1".into(),
            outcome: ResultOutcomeKind::Finished,
            state: ResultState::Reviewing,
            worktree_path: Some("C:/wt".into()),
            branch: None,
            base_oid: None,
            head_oid: None,
            changed_paths: paths
                .iter()
                .map(|p| ResultChangedPath {
                    path: (*p).to_string(),
                    old_path: None,
                    status: "M".into(),
                })
                .collect(),
            checks: Vec::new(),
            commit: None,
            openspec_change_id: None,
            linked_execution_ids: Vec::new(),
            updated_at: "2026-01-01T00:00:00Z".into(),
        }
    }

    #[test]
    fn no_condition_and_report_result_both_just_finish() {
        assert!(judge(None, &[], None).is_met());
        assert!(judge(Some(&CompletionCondition::ReportsResult), &[], None).is_met());
    }

    /// The failure this whole module exists for: a helper asked to make the
    /// tests pass, which stopped without ever running them, used to count as
    /// finished.
    #[test]
    fn a_check_that_was_never_run_is_not_a_pass() {
        let condition = CompletionCondition::ChecksPass {
            command: "cargo test".into(),
        };
        let verdict = judge(Some(&condition), &[], None);
        assert_eq!(
            verdict,
            CompletionVerdict::Unmet {
                reason: "It was asked to make \"cargo test\" pass, but it never ran that check.".into()
            }
        );
    }

    #[test]
    fn a_failing_check_is_not_a_pass_and_a_passing_one_is() {
        let condition = CompletionCondition::ChecksPass {
            command: "cargo test".into(),
        };
        let failed = judge(Some(&condition), &[check("cargo test", CheckRunOutcome::Failed)], None);
        assert!(matches!(failed, CompletionVerdict::Unmet { .. }));
        let passed = judge(Some(&condition), &[check("cargo test", CheckRunOutcome::Passed)], None);
        assert!(passed.is_met());
    }

    /// A helper that broke the tests, fixed them, and re-ran has met the
    /// condition: the last run is what stands.
    #[test]
    fn the_last_run_of_a_check_is_the_one_that_counts() {
        let condition = CompletionCondition::ChecksPass {
            command: "cargo test".into(),
        };
        let checks = vec![
            check("cargo test", CheckRunOutcome::Failed),
            check("cargo test", CheckRunOutcome::Passed),
        ];
        assert!(judge(Some(&condition), &checks, None).is_met());
    }

    /// The recorded name rarely matches the condition word for word.
    #[test]
    fn a_check_is_recognised_when_its_recorded_name_is_dressed_up() {
        let condition = CompletionCondition::ChecksPass {
            command: "cargo test".into(),
        };
        for name in ["cargo test", "cargo test --lib", "Cargo Test"] {
            assert!(
                judge(Some(&condition), &[check(name, CheckRunOutcome::Passed)], None).is_met(),
                "{name} should count"
            );
        }
        // But an unrelated check does not stand in for the one asked for.
        let other = judge(Some(&condition), &[check("npm run lint", CheckRunOutcome::Passed)], None);
        assert!(matches!(other, CompletionVerdict::Unmet { .. }));
    }

    #[test]
    fn files_that_were_not_changed_are_named_in_the_reason() {
        let condition = CompletionCondition::FilesChanged {
            paths: vec!["src/parser.rs".into(), "src/lexer.rs".into()],
        };
        let verdict = judge(Some(&condition), &[], Some(&result_with(&["src/parser.rs"])));
        match verdict {
            CompletionVerdict::Unmet { reason } => {
                assert!(reason.contains("src/lexer.rs"), "{reason}");
                assert!(!reason.contains("src/parser.rs"), "only the missing one: {reason}");
            }
            other => panic!("expected Unmet, got {other:?}"),
        }
    }

    #[test]
    fn a_folder_or_glob_is_satisfied_by_anything_inside_it() {
        let condition = CompletionCondition::FilesChanged {
            paths: vec!["src/parser/**".into()],
        };
        assert!(judge(Some(&condition), &[], Some(&result_with(&["src/parser/lex.rs"]))).is_met());
        // Windows separators in the result still match a forward-slashed
        // condition; the two come from different layers.
        assert!(judge(Some(&condition), &[], Some(&result_with(&["src\\parser\\lex.rs"]))).is_met());
        // A sibling folder does not.
        let miss = judge(Some(&condition), &[], Some(&result_with(&["src/parser2/lex.rs"])));
        assert!(matches!(miss, CompletionVerdict::Unmet { .. }));
    }

    #[test]
    fn a_helper_that_changed_nothing_cannot_satisfy_a_file_condition() {
        let condition = CompletionCondition::FilesChanged {
            paths: vec!["src/parser.rs".into()],
        };
        let verdict = judge(Some(&condition), &[], None);
        match verdict {
            CompletionVerdict::Unmet { reason } => assert!(reason.contains("no changes at all"), "{reason}"),
            other => panic!("expected Unmet, got {other:?}"),
        }
    }
}
