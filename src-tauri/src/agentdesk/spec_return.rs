//! Turning finished work back into a spec update.
//!
//! An OpenSpec change starts work, and until now that was the whole
//! relationship: the spec was a launch source and nothing came back. What the
//! agent actually did stayed in the chat, so the spec drifted from the code
//! the moment the work landed, and someone had to notice and hand-edit it.
//!
//! This closes the loop. A finished session that came from an OpenSpec change
//! knows what it changed (the result record's paths), what it was asked to do
//! (the source), and what it said about the work (its own last message). That
//! is enough to write the instruction a spec edit needs.
//!
//! What this module does NOT do is write anything. It builds an instruction
//! and hands it to `openspec::edit_draft`, the same drafter a hand-written
//! instruction uses, which returns a proposed file body for the person to read
//! and save. One write path, one refusal, no second way for an agent to reach
//! the spec files: the agent's work is evidence, and a person still decides
//! what the spec says.

use crate::agentdesk::model::{AgentSession, MessageKind, MessageRole, SessionSource};
use crate::agentdesk::result::ResultRecord;

/// Which file of the change a return draft is aimed at.
///
/// Deliberately a closed set rather than a free path: these are the three
/// files a finished piece of work has something to say about, and each has a
/// different question behind it. Anything else is a hand edit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum SpecReturnTarget {
    /// Tick off what the work completed, and add any step it turned out to
    /// need. The most common return: the tasks list is what goes stale first.
    Tasks,
    /// Record what the work revealed about how the change actually behaves.
    Proposal,
    /// Record a decision the work forced, or a design note it invalidated.
    Design,
}

impl SpecReturnTarget {
    /// The file this target edits, relative to the change folder.
    pub fn file(self) -> &'static str {
        match self {
            SpecReturnTarget::Tasks => "tasks.md",
            SpecReturnTarget::Proposal => "proposal.md",
            SpecReturnTarget::Design => "design.md",
        }
    }

    /// What the drafter is being asked to do, in the words of this target.
    fn ask(self) -> &'static str {
        match self {
            SpecReturnTarget::Tasks => {
                "Tick off the steps this work completed, and add any step the work turned out to \
need that is not listed yet. Do not tick a step the work did not actually finish, and do not \
remove steps that are still open."
            }
            SpecReturnTarget::Proposal => {
                "Update the proposal to match what the work found. Correct anything the work \
proved wrong, and note anything it revealed that the proposal does not mention. Keep the \
proposal's own voice and structure."
            }
            SpecReturnTarget::Design => {
                "Record the decisions this work forced, and correct any design note it proved \
wrong. Say why, not just what. Keep the document's existing structure."
            }
        }
    }
}

/// Why a session cannot send anything back to its spec.
///
/// Each is a different thing for the person to do, so each is its own
/// variant rather than one "cannot" with a sentence attached.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReturnRefusal {
    /// The session did not come from an OpenSpec change, so there is no spec
    /// to update.
    NotFromASpec,
    /// The work has not finished, so what it did is not settled yet.
    StillWorking,
    /// It finished without changing anything, so there is nothing to report.
    NothingChanged,
}

impl ReturnRefusal {
    /// The sentence a person reads. Says what is true and what would change
    /// it, never just "unavailable".
    pub fn plain(&self) -> &'static str {
        match self {
            ReturnRefusal::NotFromASpec => {
                "This chat did not come from a spec change, so there is nothing to update."
            }
            ReturnRefusal::StillWorking => {
                "Wait for the work to finish, then send what it found back to the spec."
            }
            ReturnRefusal::NothingChanged => {
                "This work did not change any files, so there is nothing to tell the spec."
            }
        }
    }
}

/// Everything a return draft needs, gathered from the session and its result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReturnContext {
    /// The change whose files are being edited.
    pub change_id: String,
    /// The file to draft, per the chosen target.
    pub file: String,
    /// The instruction handed to the drafter.
    pub instruction: String,
}

/// Builds the instruction for one return draft, or says why there is none.
///
/// The instruction is assembled from facts, not from asking the agent to
/// summarise itself again: the paths come from the result record and the
/// account of the work comes from what the agent already said. A model that
/// exaggerated what it did cannot make the file list say so.
pub fn build_return_context(
    session: &AgentSession,
    result: Option<&ResultRecord>,
    target: SpecReturnTarget,
) -> Result<ReturnContext, ReturnRefusal> {
    let change_id = match &session.header.source {
        SessionSource::OpenSpecChange { change_id, .. }
        | SessionSource::OpenSpecTask { change_id, .. } => change_id.clone(),
        _ => return Err(ReturnRefusal::NotFromASpec),
    };

    let result = result.ok_or(ReturnRefusal::StillWorking)?;
    if result.changed_paths.is_empty() {
        return Err(ReturnRefusal::NothingChanged);
    }

    let mut instruction = String::new();
    instruction.push_str(target.ask());
    instruction.push_str("\n\nThis is what an AI agent just did for this change.\n\n");

    if let SessionSource::OpenSpecTask { task_text, .. } = &session.header.source {
        if !task_text.trim().is_empty() {
            instruction.push_str("The step it was working on:\n");
            instruction.push_str(task_text.trim());
            instruction.push_str("\n\n");
        }
    }

    instruction.push_str("Files it changed:\n");
    for path in &result.changed_paths {
        instruction.push_str("- ");
        instruction.push_str(&path.path);
        instruction.push('\n');
    }

    if let Some(said) = closing_account(session) {
        instruction.push_str("\nWhat it said about the work:\n");
        instruction.push_str(&said);
        instruction.push('\n');
    }

    // The one rule that matters for a spec file: the agent's account is a
    // claim, the file list is a fact, and the spec must not gain anything
    // neither of them supports.
    instruction.push_str(
        "\nOnly write what these files and this account actually support. Do not invent work \
that is not listed here, and do not claim something is done if the account does not say so.",
    );

    Ok(ReturnContext {
        change_id,
        file: target.file().to_string(),
        instruction,
    })
}

/// The agent's own closing account of the work: its last piece of prose.
///
/// Tool rows, approvals and thought summaries are skipped: they say what it
/// touched, which the file list already covers honestly, and none of them is
/// the agent explaining itself.
fn closing_account(session: &AgentSession) -> Option<String> {
    let text = session
        .messages
        .iter()
        .rev()
        .find(|m| {
            m.role == MessageRole::Assistant
                && matches!(m.kind, MessageKind::Assistant | MessageKind::Result)
                && !m.plain_content.trim().is_empty()
        })
        .map(|m| m.plain_content.trim().to_string())?;

    // Long enough to carry the reasoning, short enough that the file being
    // edited is still the bulk of what the drafter reads.
    const MAX_ACCOUNT_CHARS: usize = 4_000;
    if text.chars().count() <= MAX_ACCOUNT_CHARS {
        return Some(text);
    }
    let kept: String = text.chars().take(MAX_ACCOUNT_CHARS).collect();
    Some(format!("{kept}\n(cut short)"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agentdesk::model::{
        AgentSessionHeader, SessionIntent, SessionMessage, SessionState, SourceSnapshot,
        CURRENT_SCHEMA_VERSION,
    };
    use crate::agentdesk::result::{ResultChangedPath, ResultOutcomeKind, ResultState};

    fn snapshot() -> SourceSnapshot {
        SourceSnapshot {
            title: "Add the thing".into(),
            summary: "A change".into(),
            captured_at: "2026-01-01T00:00:00Z".into(),
            live_unavailable: false,
        }
    }

    fn session(source: SessionSource) -> AgentSession {
        AgentSession::new(AgentSessionHeader {
            schema_version: CURRENT_SCHEMA_VERSION,
            session_id: "sess-1".into(),
            repo_id: "repo-1".into(),
            repo_path: "C:/code/widgets".into(),
            repo_name: "widgets".into(),
            title: "Add the thing".into(),
            source,
            intent: SessionIntent::Fix,
            state: SessionState::Finished,
            created_at: "2026-01-01T00:00:00Z".into(),
            updated_at: "2026-01-01T00:00:00Z".into(),
            unread: false,
            changed_file_count: 2,
            active_execution_id: None,
            archived: false,
            graph_started_at: None,
            preferred_provider: None,
            preferred_mode: None,
            preferred_team: None,
        })
    }

    fn task_session() -> AgentSession {
        session(SessionSource::OpenSpecTask {
            change_id: "add-the-thing".into(),
            task_index: 3,
            task_text: "Write the parser".into(),
            snapshot: snapshot(),
        })
    }

    fn result_with(paths: &[&str]) -> ResultRecord {
        ResultRecord {
            execution_id: "exec-1".into(),
            outcome: ResultOutcomeKind::Finished,
            state: ResultState::Reviewing,
            worktree_path: Some("C:/wt/exec-1".into()),
            branch: Some("agent-desk/exec-1".into()),
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
            openspec_change_id: Some("add-the-thing".into()),
            linked_execution_ids: Vec::new(),
            updated_at: "2026-01-01T00:00:00Z".into(),
        }
    }

    fn assistant(text: &str, kind: MessageKind) -> SessionMessage {
        SessionMessage {
            message_id: format!("m-{text:.8}"),
            segment_id: "seg-1".into(),
            role: MessageRole::Assistant,
            timestamp: "2026-01-01T00:00:00Z".into(),
            plain_content: text.into(),
            rendered_content: None,
            provider: None,
            model: None,
            kind,
            execution_id: Some("exec-1".into()),
            sequence: None,
            import: None,
            targets: Vec::new(),
        }
    }

    #[test]
    fn a_chat_that_did_not_come_from_a_spec_has_nothing_to_send_back() {
        let s = session(SessionSource::Manual {
            repo_id: "repo-1".into(),
        });
        assert_eq!(
            build_return_context(&s, Some(&result_with(&["a.rs"])), SpecReturnTarget::Tasks),
            Err(ReturnRefusal::NotFromASpec)
        );
    }

    #[test]
    fn work_that_has_not_finished_or_changed_nothing_is_refused_separately() {
        let s = task_session();
        assert_eq!(
            build_return_context(&s, None, SpecReturnTarget::Tasks),
            Err(ReturnRefusal::StillWorking),
            "no result yet means the work is not settled"
        );
        assert_eq!(
            build_return_context(&s, Some(&result_with(&[])), SpecReturnTarget::Tasks),
            Err(ReturnRefusal::NothingChanged)
        );
        // Each refusal says what would change it, rather than just "no".
        assert!(ReturnRefusal::StillWorking.plain().contains("Wait for the work"));
        assert!(ReturnRefusal::NothingChanged.plain().contains("did not change any files"));
    }

    #[test]
    fn the_instruction_carries_the_step_the_files_and_the_agents_account() {
        let mut s = task_session();
        s.messages.push(assistant("Ran a search", MessageKind::Tool));
        s.messages.push(assistant(
            "I wrote the parser and found the format is not what the proposal assumed.",
            MessageKind::Assistant,
        ));
        let ctx = build_return_context(
            &s,
            Some(&result_with(&["src/parser.rs", "src/parser.test.rs"])),
            SpecReturnTarget::Tasks,
        )
        .expect("a finished spec task can report back");

        assert_eq!(ctx.change_id, "add-the-thing");
        assert_eq!(ctx.file, "tasks.md");
        assert!(ctx.instruction.contains("Tick off the steps"));
        assert!(ctx.instruction.contains("Write the parser"), "the step it worked on");
        assert!(ctx.instruction.contains("- src/parser.rs"));
        assert!(ctx.instruction.contains("- src/parser.test.rs"));
        assert!(ctx.instruction.contains("not what the proposal assumed"));
        // Tool rows are activity, not an account of the work.
        assert!(!ctx.instruction.contains("Ran a search"));
        // The guard against a spec gaining work nobody did.
        assert!(ctx.instruction.contains("Do not invent work"));
    }

    #[test]
    fn each_target_edits_its_own_file_and_asks_its_own_question() {
        let s = task_session();
        let r = result_with(&["src/parser.rs"]);
        for (target, file, needle) in [
            (SpecReturnTarget::Tasks, "tasks.md", "Tick off"),
            (SpecReturnTarget::Proposal, "proposal.md", "Update the proposal"),
            (SpecReturnTarget::Design, "design.md", "decisions this work forced"),
        ] {
            let ctx = build_return_context(&s, Some(&r), target).expect("built");
            assert_eq!(ctx.file, file);
            assert!(ctx.instruction.contains(needle), "{target:?}: {}", ctx.instruction);
        }
    }

    #[test]
    fn a_very_long_account_is_cut_rather_than_swamping_the_file() {
        let mut s = task_session();
        s.messages
            .push(assistant(&"x".repeat(9_000), MessageKind::Assistant));
        let ctx =
            build_return_context(&s, Some(&result_with(&["a.rs"])), SpecReturnTarget::Tasks).unwrap();
        assert!(ctx.instruction.contains("(cut short)"));
        assert!(ctx.instruction.chars().count() < 6_000);
    }

    /// A change-sourced session (not a single task) can report back too; it
    /// just has no one step to name.
    #[test]
    fn a_change_sourced_chat_reports_back_without_a_step() {
        let s = session(SessionSource::OpenSpecChange {
            change_id: "add-the-thing".into(),
            snapshot: snapshot(),
        });
        let ctx =
            build_return_context(&s, Some(&result_with(&["a.rs"])), SpecReturnTarget::Proposal).unwrap();
        assert_eq!(ctx.change_id, "add-the-thing");
        assert!(ctx.instruction.contains("- a.rs"));
    }
}
