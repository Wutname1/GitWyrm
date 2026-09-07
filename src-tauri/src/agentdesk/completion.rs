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
    ///
    /// `matched_loosely` names the check that answered the condition when it
    /// was not called what the condition asked for -- `cargo test --lib` for
    /// `cargo test`, say. Met either way: a helper that ran more than it was
    /// asked to has still done the job. But the two are not the same claim,
    /// and a met condition is otherwise completely silent, so the difference
    /// is carried out rather than dropped here.
    Met { matched_loosely: Option<LooseMatch> },
    /// The helper stopped without meeting it. `reason` is the sentence shown
    /// in the chat, written for someone who did not read the transcript.
    Unmet { reason: String },
}

/// A check that satisfied a condition under a different name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LooseMatch {
    /// What the condition asked for.
    pub wanted: String,
    /// What the check that answered it was actually called.
    pub recorded: String,
}

impl CompletionVerdict {
    /// The plain [`CompletionVerdict::Met`] -- nothing to remark on.
    pub fn met() -> Self {
        CompletionVerdict::Met { matched_loosely: None }
    }

    pub fn is_met(&self) -> bool {
        matches!(self, CompletionVerdict::Met { .. })
    }

    /// The sentence to add to the chat, if this verdict has anything to say
    /// beyond passing or failing. `None` when there is nothing worth a note.
    pub fn note(&self) -> Option<String> {
        match self {
            CompletionVerdict::Met {
                matched_loosely: Some(LooseMatch { wanted, recorded }),
            } => Some(format!(
                "It was asked to make \"{wanted}\" pass. The check that passed was called \"{recorded}\"."
            )),
            _ => None,
        }
    }
}

/// Judges one helper against its own completion condition.
///
/// `checks` are the checks that helper ran; `result` is its result record, if
/// one was built. Both are what the run itself produced -- this never asks
/// the model, in words, whether it thinks it did the job, because a helper's
/// own summary of itself is exactly what a condition exists to check.
///
/// How far that goes is worth being exact about, because the comment used to
/// claim more than the code does. A check arrives as
/// `RunStep::Check { name, passed, .. }`, and BOTH fields are the helper's
/// own report -- nothing here re-runs the check to see for itself, by
/// deliberate choice (see this module's own doc comment on why). So a helper
/// that reported a check it never ran would be believed.
///
/// What this does verify is that the report is *about the right thing*: that
/// a check answering "make `cargo test` pass" is actually named that, rather
/// than being any string that happens to sit inside it. That is a smaller
/// claim than "the check really passed", and the difference should stay
/// visible to whoever reads this next.
pub fn judge(
    condition: Option<&CompletionCondition>,
    checks: &[ResultCheckOutcome],
    result: Option<&ResultRecord>,
) -> CompletionVerdict {
    let Some(condition) = condition else {
        // No condition recorded: an older session, or the lead itself.
        // Finishing is finishing.
        return CompletionVerdict::met();
    };

    match condition {
        // The helper finishing IS the report. This is the condition every
        // proposal uses today, and it is deliberately the weakest one.
        CompletionCondition::ReportsResult => CompletionVerdict::met(),

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
                CheckRunOutcome::Passed => CompletionVerdict::Met {
                    // Only remarked on when the names genuinely differ. An
                    // exact match has nothing to explain.
                    matched_loosely: if last.command_name.trim().eq_ignore_ascii_case(command.trim()) {
                        None
                    } else {
                        Some(LooseMatch {
                            wanted: command.clone(),
                            recorded: last.command_name.clone(),
                        })
                    },
                },
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
            // GitWyrm never managed to look inside the helper's folder, so it
            // does not know whether these files changed. Still unmet -- a
            // condition nobody could check is not a condition met, and
            // nothing should land on the strength of a guess -- but the
            // sentence says what actually happened instead of blaming the
            // helper for GitWyrm's blindness.
            if result.changed_paths_unreadable.is_some() {
                let named = paths
                    .iter()
                    .map(|p| p.as_str())
                    .collect::<Vec<_>>()
                    .join(", ");
                return CompletionVerdict::Unmet {
                    reason: format!(
                        "GitWyrm could not read this helper's folder, so it cannot tell whether {named} changed."
                    ),
                };
            }
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
                return CompletionVerdict::met();
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
///
/// Loose in one direction only. The recorded name may add words; it may not
/// drop them. This used to accept either name containing the other, which
/// meant a *shorter* recorded name satisfied a longer condition: a check
/// called `cargo` answered "make `cargo test --all-features` pass", `t`
/// answered `cargo test`, and an empty name answered every condition there
/// is, because every string contains the empty one.
///
/// That matters more than a string rule usually would. The name is the
/// helper's own account of what it ran -- and a met condition is silent,
/// while an unmet one stops the work and says so. So the weakest possible
/// name bought the strongest possible outcome: the helper's changes went on
/// to be merged with nothing said, which is the exact case
/// `enforce_completion_condition` exists to prevent.
///
/// Compared as whole words rather than as substrings, so a shorter name can
/// no longer ride inside a longer one. Punctuation is trimmed from each word
/// so the `Tests (cargo test)` shape above still matches -- without that, its
/// words are `(cargo` and `test)` and it would stop matching.
fn check_matches(recorded: &str, wanted: &str) -> bool {
    let recorded = words(recorded);
    let wanted = words(wanted);
    if recorded.is_empty() || wanted.is_empty() {
        return false;
    }
    wanted.iter().all(|w| recorded.contains(w))
}

/// A name split into comparable words, lowercased, with surrounding
/// punctuation dropped and empty pieces discarded.
fn words(name: &str) -> Vec<String> {
    name.split_whitespace()
        .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric()).to_lowercase())
        .filter(|w| !w.is_empty())
        .collect()
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
            changed_paths_unreadable: None,
            linked_execution_ids: Vec::new(),
            updated_at: "2026-01-01T00:00:00Z".into(),
        }
    }

    /// A result whose worktree GitWyrm could not read: an empty file list
    /// that is an absence of knowledge rather than an absence of changes.
    fn result_unreadable() -> ResultRecord {
        let mut r = result_with(&[]);
        r.changed_paths_unreadable = Some("could not open worktree".into());
        r
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
    /// A name shorter than the one asked for cannot stand in for it.
    ///
    /// The old rule accepted either name containing the other, so a check
    /// called `cargo` answered a condition asking for `cargo test
    /// --all-features`, `t` answered `cargo test`, and an empty name answered
    /// everything -- every string contains the empty one. The name is the
    /// helper's own account of what it ran, and a met condition is silent
    /// while an unmet one stops the work, so the weakest name bought the
    /// strongest outcome.
    #[test]
    fn a_shorter_name_cannot_stand_in_for_the_one_asked_for() {
        let condition = CompletionCondition::ChecksPass {
            command: "cargo test --all-features".into(),
        };
        for name in ["", "   ", "t", "cargo", "test"] {
            let verdict = judge(Some(&condition), &[check(name, CheckRunOutcome::Passed)], None);
            assert!(
                matches!(verdict, CompletionVerdict::Unmet { .. }),
                "{name:?} must not satisfy a condition it does not name"
            );
        }
    }

    /// The doc comment's own example keeps working.
    ///
    /// Words are compared with surrounding punctuation trimmed precisely so
    /// this shape still matches -- untrimmed, its words are `(cargo` and
    /// `test)` and a helper that ran exactly what was asked would be failed.
    #[test]
    fn a_decorated_name_still_matches_the_check_it_names() {
        let condition = CompletionCondition::ChecksPass {
            command: "cargo test".into(),
        };
        for name in ["Tests (cargo test)", "cargo test --lib", "Cargo Test", "cargo test"] {
            assert!(
                judge(Some(&condition), &[check(name, CheckRunOutcome::Passed)], None).is_met(),
                "{name} should count"
            );
        }
    }

    /// A check called something other than what was asked for still counts,
    /// and still gets said out loud.
    #[test]
    fn a_differently_named_check_is_met_and_remarked_on() {
        let condition = CompletionCondition::ChecksPass {
            command: "cargo test".into(),
        };
        let verdict = judge(
            Some(&condition),
            &[check("cargo test --lib", CheckRunOutcome::Passed)],
            None,
        );
        assert!(verdict.is_met());
        let note = verdict.note().expect("a differently named check is worth a sentence");
        assert!(note.contains("cargo test --lib"), "{note}");
        assert!(note.contains("cargo test"), "{note}");
    }

    /// The ordinary case stays silent. A check called exactly what the
    /// condition asked for has nothing to explain, and a note on every
    /// passing helper would be noise.
    #[test]
    fn an_exactly_named_check_says_nothing_extra() {
        let condition = CompletionCondition::ChecksPass {
            command: "cargo test".into(),
        };
        let verdict = judge(
            Some(&condition),
            &[check("Cargo Test", CheckRunOutcome::Passed)],
            None,
        );
        assert!(verdict.is_met());
        assert_eq!(verdict.note(), None, "only the spelling differed");
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

    /// A folder GitWyrm could not read is not a folder with no changes in it.
    ///
    /// The changed-file list is measured from git, and a failure to measure
    /// used to be recorded as an empty list -- so a helper that changed
    /// exactly what it was asked to could be failed, and told in the chat
    /// that it "left no changes at all". That is an accusation about the
    /// helper assembled out of GitWyrm's own failure to open a directory.
    ///
    /// Still unmet: a condition nobody could check is not a condition met,
    /// and nothing should be merged on a guess. Only the sentence changes,
    /// and the sentence is the whole harm.
    #[test]
    fn an_unreadable_folder_is_not_reported_as_no_changes() {
        let condition = CompletionCondition::FilesChanged {
            paths: vec!["src/parser.rs".into()],
        };
        let verdict = judge(Some(&condition), &[], Some(&result_unreadable()));
        match verdict {
            CompletionVerdict::Unmet { reason } => {
                assert!(
                    reason.contains("could not read"),
                    "should say GitWyrm could not look: {reason}"
                );
                assert!(
                    !reason.contains("left no changes"),
                    "must not accuse the helper of changing nothing: {reason}"
                );
                assert!(reason.contains("src/parser.rs"), "{reason}");
            }
            other => panic!("expected Unmet, got {other:?}"),
        }
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
