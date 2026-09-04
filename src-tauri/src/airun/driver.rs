//! What a run tells the console, and what the console can tell it back.
//!
//! Every event carries its own `summary`: the one plain-language sentence the
//! stream row, the main-window card, and the status bar all render. Written
//! once here so those three surfaces cannot drift into describing the same run
//! differently.

use serde::{Deserialize, Serialize};
use specta::Type;

/// Where a run is, as the state pill shows it.
///
/// The glyphs matter: "Needs you" sits next to a change-status "Needs review"
/// in the same window, and colour alone does not separate them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum RunState {
    /// Getting ready: reading the plan, the deltas, the task.
    Preparing,
    /// Working. Glyph ●.
    Working,
    /// Paused at a gate, waiting for an answer. Glyph ⏸.
    NeedsYou,
    /// Done, changes waiting to be kept or undone. Glyph ✓.
    Finished,
    /// The user stopped it. Glyph ■.
    Stopped,
    /// It could not continue. Glyph ✕.
    Failed,
}

impl RunState {
    /// Whether Stop should be offered.
    pub fn is_active(self) -> bool {
        matches!(
            self,
            RunState::Preparing | RunState::Working | RunState::NeedsYou
        )
    }
}

/// One preflight check, shown before any work starts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct PreflightItem {
    pub label: String,
    /// False when the step genuinely had nothing to do -- "no spec changes to
    /// read" is said out loud rather than shown as a tick that implies work.
    pub done: bool,
    pub detail: String,
}

/// Something the run wants to do that needs an answer first.
///
/// Only side effects beyond in-repo edits and the project's own checks appear
/// here. Editing source and running tests is the job, not a gate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum GateRequest {
    AddDependency {
        name: String,
    },
    RunInstall {
        command: String,
    },
    NetworkAccess {
        target: String,
    },
    DeleteFiles {
        paths: Vec<String>,
    },
    OutsideRepo {
        path: String,
    },
    /// Sending work off this machine -- a push, a pull-request action, a
    /// posted comment or review, a merge.
    ///
    /// Its own variant because the product's promise is that the agent never
    /// silently pushes, posts, merges or changes an external service. Nothing
    /// in the capability table can enforce that: a provider asks to run a
    /// shell command, which classifies as an ordinary write, so `git push`
    /// and `sed -i` arrive at the gate looking identical. Naming the
    /// consequence is what makes the person's approval an informed one.
    Publish {
        /// What the command does, in the user's terms ("send commits to
        /// GitHub"), not the command line itself.
        effect: String,
    },
    /// Something the agent asked for that GitWyrm cannot classify.
    ///
    /// Exists so an unrecognised request is shown as what it is rather than
    /// squeezed into a variant that would mislabel the consequence -- an
    /// arbitrary ask rendered as "Install packages?" tells the user the wrong
    /// thing about what they are approving.
    Unclassified {
        summary: String,
    },
}

impl GateRequest {
    /// The card's title: the consequence, not the mechanism.
    pub fn title(&self) -> String {
        match self {
            GateRequest::AddDependency { name } => format!("Add the {name} library?"),
            GateRequest::RunInstall { .. } => "Install packages?".into(),
            GateRequest::NetworkAccess { target } => format!("Let this reach {target}?"),
            GateRequest::DeleteFiles { paths } => {
                if paths.len() == 1 {
                    format!("Delete {}?", paths[0])
                } else {
                    format!("Delete {} files?", paths.len())
                }
            }
            GateRequest::OutsideRepo { path } => format!("Touch {path}, outside this folder?"),
            GateRequest::Publish { effect } => format!("Send this out: {effect}?"),
            GateRequest::Unclassified { summary } => format!("Allow this: {summary}?"),
        }
    }

    /// Reads a provider's own one-line summary and, when it describes work
    /// leaving this machine, returns the [`GateRequest::Publish`] that names
    /// that consequence.
    ///
    /// Deliberately conservative in one direction only: a missed publish is
    /// shown as `Unclassified`, which still gates the run and still shows the
    /// raw summary, so the failure mode is a vaguer card rather than a silent
    /// publish. Matching is done on whole words so that `git push` is caught
    /// while a path like `src/pusher.rs` is not.
    pub fn classify(summary: &str) -> GateRequest {
        let lower = summary.to_lowercase();
        let words: Vec<&str> = lower.split(|c: char| !c.is_ascii_alphanumeric()).filter(|w| !w.is_empty()).collect();
        let has = |w: &str| words.iter().any(|x| *x == w);
        let has_pair = |a: &str, b: &str| words.windows(2).any(|p| p[0] == a && p[1] == b);

        // `git push`, and the force variants that arrive as the same word.
        if has("git") && has("push") {
            return GateRequest::Publish { effect: "send commits to the server".into() };
        }
        // Host CLIs: `gh pr create|merge`, `gh pr comment`, `glab mr merge`.
        if has("gh") || has("glab") {
            if has("merge") {
                return GateRequest::Publish { effect: "merge a pull request".into() };
            }
            if has("comment") || has("review") {
                return GateRequest::Publish { effect: "post a comment on the project's website".into() };
            }
            if has("create") || has("close") || has("edit") {
                return GateRequest::Publish { effect: "change a pull request or issue on the project's website".into() };
            }
        }
        // Plain-language phrasings a provider may use instead of a command.
        if has_pair("push", "to") || has_pair("publish", "to") {
            return GateRequest::Publish { effect: "send this work to the server".into() };
        }

        GateRequest::Unclassified { summary: summary.to_string() }
    }
}

/// What the user chose at a gate.
///
/// Exactly three. There is no "don't ask again" and nothing to type: a
/// remembered approval is a decision made once and applied to situations the
/// user never saw.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum GateAnswer {
    AllowOnce,
    FindAnotherWay,
    StopRun,
}

/// One row of the activity stream.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum RunStep {
    /// The preflight checklist, emitted once before work starts.
    Preflight { items: Vec<PreflightItem> },
    /// What it intends to do next.
    Plan { text: String },
    /// A file changed, with counts for the row and a path for View diff.
    Edit {
        path: String,
        added: u32,
        removed: u32,
    },
    /// One of the project's checks ran.
    Check {
        name: String,
        passed: bool,
        detail: String,
    },
    /// A gate opened. The run is paused until it is answered.
    Gate { request: GateRequest },
    /// The user's steering note, echoed back so it is visibly received.
    YouSaid { text: String },
    /// Anything the run wants to say that is not one of the above.
    Note { text: String },
    /// A tool the agent is running (searching, reading, editing files
    /// through its own tool calls) as opposed to something it is saying.
    /// Kept distinct from `Note` so tool activity never coalesces with, or
    /// reads as, the agent's own prose -- see `agentdesk::bridge::map_run_step`
    /// and `agentdesk::bridge::message_kind_for_step`, which route this to
    /// `MessageKind::Tool` (the compact activity feed) instead of the
    /// transcript's assistant messages.
    Activity { text: String },
    /// It could not do something, and is adapting.
    Adapted { text: String },
    /// What the turn cost, as the provider reported it.
    ///
    /// Not a transcript message: it updates the execution's usage total and
    /// is never shown as a row in the conversation. Emitted at most once per
    /// turn, and only when the provider actually reported numbers -- a
    /// provider that reports nothing produces no `Usage` step at all, which
    /// is what keeps "not measured" distinguishable from "cost zero".
    Usage {
        usage: crate::agentdesk::model::TurnUsage,
    },
    /// How full the model's context window is for this session.
    ///
    /// Not a transcript row and not a running total: this is occupancy, it
    /// replaces whatever was recorded before, and it goes DOWN when the agent
    /// compacts its history. Distinct from `Usage`, which is spend and does
    /// accumulate.
    ContextUsage {
        used: u32,
        size: u32,
        cost_micro_usd: Option<u32>,
    },
    /// The run ended.
    Ended { state: RunState, detail: String },
}

/// An event as it reaches the UI.
///
/// `summary` is the sentence every surface renders. `repo_id` and `session_id`
/// are what keep an ended run from writing into a newer one's console.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct RunEventKind {
    pub repo_id: String,
    pub session_id: String,
    pub state: RunState,
    pub summary: String,
    pub step: RunStep,
}

/// The seam the console talks to.
///
/// A trait so the console can be built and verified against a scripted driver
/// before the engine is wired -- and so the engine, when it lands, changes
/// nothing above this line.
pub trait RunDriver: Send + Sync {
    /// Begins the run. Events arrive through the session's emitter.
    fn start(&mut self);
    /// Answers the open gate. Ignored when none is open.
    fn answer_gate(&mut self, answer: GateAnswer);
    /// Queues a steering note. Does not interrupt the current step.
    fn note(&mut self, text: String);
    /// Asks the run to stop. Must land within a step, not at the end of a run.
    fn stop(&mut self);
}

/// Builds the sentence for an edit row.
pub fn edit_summary(path: &str, added: u32, removed: u32) -> String {
    match (added, removed) {
        (0, 0) => format!("Left {path} unchanged"),
        (a, 0) => format!("Added {a} {} to {path}", lines(a)),
        (0, r) => format!("Removed {r} {} from {path}", lines(r)),
        (a, r) => format!("Changed {path}: {a} {} added, {r} removed", lines(a)),
    }
}

fn lines(n: u32) -> &'static str {
    if n == 1 {
        "line"
    } else {
        "lines"
    }
}

/// The sentence for any step, so no surface writes its own.
pub fn summarize(step: &RunStep) -> String {
    match step {
        RunStep::Preflight { items } => {
            let done = items.iter().filter(|i| i.done).count();
            format!("Got ready: {done} of {} checks", items.len())
        }
        RunStep::Plan { text } => text.clone(),
        RunStep::Edit {
            path,
            added,
            removed,
        } => edit_summary(path, *added, *removed),
        RunStep::Check { name, passed, .. } => {
            if *passed {
                format!("{name} passed")
            } else {
                format!("{name} failed")
            }
        }
        RunStep::Gate { request } => request.title(),
        RunStep::YouSaid { text } => format!("You said: {text}"),
        RunStep::Note { text } => text.clone(),
        RunStep::Activity { text } => text.clone(),
        RunStep::Adapted { text } => text.clone(),
        // Never rendered as a transcript row (see `RunStep::Usage`), so this
        // sentence exists only for logs and for the exhaustiveness this match
        // enforces.
        RunStep::Usage { .. } => "Recorded what the turn cost".to_string(),
        RunStep::ContextUsage { used, size, .. } => {
            format!("Context window: {used} of {size}")
        }
        RunStep::Ended { detail, .. } => detail.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stop_is_offered_exactly_while_a_run_is_live() {
        assert!(RunState::Preparing.is_active());
        assert!(RunState::Working.is_active());
        assert!(
            RunState::NeedsYou.is_active(),
            "a paused run can still be stopped"
        );
        assert!(!RunState::Finished.is_active());
        assert!(!RunState::Stopped.is_active());
        assert!(!RunState::Failed.is_active());
    }

    #[test]
    fn gate_titles_lead_with_the_consequence() {
        let g = GateRequest::AddDependency {
            name: "left-pad".into(),
        };
        assert!(g.title().contains("left-pad"));
    }

    #[test]
    fn deleting_one_file_names_it_and_many_counts_them() {
        let one = GateRequest::DeleteFiles {
            paths: vec!["a.txt".into()],
        };
        assert!(one.title().contains("a.txt"));
        let many = GateRequest::DeleteFiles {
            paths: vec!["a".into(), "b".into(), "c".into()],
        };
        assert!(many.title().contains('3'));
    }

    #[test]
    fn every_step_produces_a_summary() {
        let steps = [
            RunStep::Preflight { items: vec![] },
            RunStep::Plan {
                text: "Read the file".into(),
            },
            RunStep::Edit {
                path: "a.rs".into(),
                added: 2,
                removed: 1,
            },
            RunStep::Check {
                name: "typecheck".into(),
                passed: true,
                detail: String::new(),
            },
            RunStep::Gate {
                request: GateRequest::RunInstall {
                    command: "npm i".into(),
                },
            },
            RunStep::YouSaid {
                text: "use tabs".into(),
            },
            RunStep::Note {
                text: "thinking".into(),
            },
            RunStep::Activity {
                text: "Finding files matching **/tasks.md".into(),
            },
            RunStep::Adapted {
                text: "did it another way".into(),
            },
            RunStep::Ended {
                state: RunState::Finished,
                detail: "Finished.".into(),
            },
        ];
        for s in &steps {
            assert!(!summarize(s).is_empty(), "{s:?} produced no sentence");
        }
    }

    #[test]
    fn a_steering_note_is_echoed_so_it_is_visibly_received() {
        let s = summarize(&RunStep::YouSaid {
            text: "use tabs".into(),
        });
        assert_eq!(s, "You said: use tabs");
    }

    #[test]
    fn edit_summaries_read_as_sentences() {
        assert_eq!(edit_summary("a.rs", 1, 0), "Added 1 line to a.rs");
        assert_eq!(edit_summary("a.rs", 3, 0), "Added 3 lines to a.rs");
        assert_eq!(edit_summary("a.rs", 0, 2), "Removed 2 lines from a.rs");
        assert_eq!(edit_summary("a.rs", 0, 0), "Left a.rs unchanged");
    }

    #[test]
    fn gate_answers_offer_no_way_to_remember() {
        let json = serde_json::to_string(&GateAnswer::AllowOnce).unwrap();
        assert_eq!(json, "\"allowOnce\"");
        for bad in ["always", "remember", "dontAsk"] {
            assert!(!json.contains(bad));
        }
    }

    #[test]
    fn preflight_can_honestly_report_nothing_to_do() {
        // "No spec changes to read" must be sayable. A tick implying work that did
        // not happen is worse than an honest empty.
        let item = PreflightItem {
            label: "Read the spec changes".into(),
            done: false,
            detail: "There are none for this task".into(),
        };
        assert!(!item.done);
        assert!(!item.detail.is_empty());
    }

    // -- GateRequest::classify: the product promises the agent never silently
    // pushes, posts, merges or changes an external service. No capability can
    // enforce that, because a publishing command arrives as an ordinary shell
    // write. Naming the consequence on the card is the enforcement. --

    #[test]
    fn a_git_push_is_named_as_sending_work_out_not_as_a_generic_ask() {
        let gate = GateRequest::classify("Run `git push origin main`");
        assert!(
            matches!(gate, GateRequest::Publish { .. }),
            "a push must be classified as publishing, got {gate:?}"
        );
        // The card must say what happens, in the user's terms.
        assert!(gate.title().contains("Send this out"), "title was {:?}", gate.title());
        assert!(gate.title().contains("commits"), "title was {:?}", gate.title());
    }

    #[test]
    fn a_force_push_is_still_a_publish() {
        assert!(matches!(
            GateRequest::classify("git push --force-with-lease origin feature"),
            GateRequest::Publish { .. }
        ));
    }

    #[test]
    fn host_cli_actions_name_the_specific_consequence() {
        let merge = GateRequest::classify("gh pr merge 318 --squash");
        assert!(matches!(&merge, GateRequest::Publish { effect } if effect.contains("merge")), "{merge:?}");

        let comment = GateRequest::classify("gh pr comment 318 --body 'looks good'");
        assert!(matches!(&comment, GateRequest::Publish { effect } if effect.contains("comment")), "{comment:?}");

        let create = GateRequest::classify("gh pr create --title 'Fix it'");
        assert!(matches!(&create, GateRequest::Publish { .. }), "{create:?}");

        // GitLab's CLI reaches the same conclusion.
        assert!(matches!(GateRequest::classify("glab mr merge 12"), GateRequest::Publish { .. }));
    }

    #[test]
    fn a_plain_language_publish_is_caught_without_a_command_line() {
        assert!(matches!(
            GateRequest::classify("Push to the remote so CI can run"),
            GateRequest::Publish { .. }
        ));
    }

    /// The half of the behaviour that keeps the classifier honest: matching on
    /// whole words means an ordinary edit that merely CONTAINS one of these
    /// words is not dressed up as a publish. Crying wolf on every file edit
    /// would teach people to click through the one card that matters.
    #[test]
    fn ordinary_work_is_not_mislabelled_as_publishing() {
        for summary in [
            "Edit src/pusher.rs",
            "Write to gh-pages/index.html",
            "Run `npm test`",
            "Delete build/output.txt",
            "Add a comment to the merge helper's docstring",
        ] {
            let gate = GateRequest::classify(summary);
            assert!(
                matches!(gate, GateRequest::Unclassified { .. }),
                "{summary:?} must not be classified as publishing, got {gate:?}"
            );
        }
    }

    #[test]
    fn an_unrecognised_ask_still_gates_and_still_shows_its_own_summary() {
        // The failure mode is a vaguer card, never a silent pass.
        let gate = GateRequest::classify("Do something GitWyrm has never seen");
        match gate {
            GateRequest::Unclassified { ref summary } => {
                assert_eq!(summary, "Do something GitWyrm has never seen");
            }
            other => panic!("expected Unclassified, got {other:?}"),
        }
        assert!(gate.title().contains("Allow this"));
    }
}
