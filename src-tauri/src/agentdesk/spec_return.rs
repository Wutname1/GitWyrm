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
    /// GitWyrm could not read the folder the work was done in, so what it
    /// changed is unknown. Distinct from `NothingChanged`, which is a
    /// measurement -- this is the absence of one.
    ChangesUnreadable,
    /// The work was thrown away with Undo, so it proved nothing the spec
    /// should be told about.
    ///
    /// The button that starts this is already meant to be unavailable for a
    /// discarded result -- but that rule was only ever enforced in the
    /// window, against a record chosen by the panel, while the backend acted
    /// on a different one. A rule about what may reach the spec has to hold
    /// where the spec is reached.
    WasThrownAway,
    /// The result has not been accepted yet, so it is still under review.
    StillBeingReviewed,
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
            ReturnRefusal::WasThrownAway => {
                "This work was undone, so there is nothing to tell the spec about it."
            }
            ReturnRefusal::StillBeingReviewed => {
                "Keep this work first, then tell the spec what it found."
            }
            ReturnRefusal::ChangesUnreadable => {
                "GitWyrm could not read the folder this work was done in, so it cannot tell the spec what changed."
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
///
/// **It reports on one execution, named.** The caller used to pick a record
/// itself -- the last one in the file -- and hand it over, with no way for
/// this function to know whether that was the execution the person was
/// actually looking at. In a session with a lead and helpers, the review
/// panel is rendered per execution, so reviewing one helper's three files
/// and pressing the button drafted a spec update from a different helper's
/// work. It looked plausible, because it was real work from the same
/// session -- just the wrong slice of it. The file list being honest about
/// what a model did is worth nothing if it is the wrong model's list.
///
/// `results` is the whole set and `execution_id` names the one under review,
/// so the choice cannot be made anywhere else.
pub fn build_return_context(
    session: &AgentSession,
    results: &[ResultRecord],
    execution_id: &str,
    target: SpecReturnTarget,
) -> Result<ReturnContext, ReturnRefusal> {
    let change_id = match &session.header.source {
        SessionSource::OpenSpecChange { change_id, .. }
        | SessionSource::OpenSpecTask { change_id, .. } => change_id.clone(),
        _ => return Err(ReturnRefusal::NotFromASpec),
    };

    let result = super::result::find_result(results, execution_id).ok_or(ReturnRefusal::StillWorking)?;

    // The spec is the project's source of truth and a mistake there outlives
    // the session, so only work the person actually accepted may reach it.
    match result.state {
        super::result::ResultState::Kept | super::result::ResultState::Committed => {}
        super::result::ResultState::Discarded => return Err(ReturnRefusal::WasThrownAway),
        _ => return Err(ReturnRefusal::StillBeingReviewed),
    }

    // Only a real measurement of nothing counts as nothing. A record whose
    // folder could not be read has an empty list for a different reason, and
    // "it finished without changing anything" would be a claim about the work
    // rather than about GitWyrm's own blindness. Keep already refuses such a
    // record, so this should be unreachable -- kept because the rule belongs
    // beside the check it guards, not in another file's ordering.
    if result.changed_paths_unreadable.is_some() {
        return Err(ReturnRefusal::ChangesUnreadable);
    }
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

    if let Some(said) = closing_account(session, execution_id) {
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
///
/// Scoped to the execution being reported on. Unscoped, a session with a
/// lead and helpers could pair one execution's file list with another's
/// prose under "What it said about the work" -- two true halves assembled
/// into an account that never happened, in text the person is invited to
/// save into the spec.
fn closing_account(session: &AgentSession, execution_id: &str) -> Option<String> {
    let text = session
        .messages
        .iter()
        .rev()
        .find(|m| {
            m.role == MessageRole::Assistant
                && m.execution_id.as_deref() == Some(execution_id)
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
        preferred_model: None,
        preferred_effort: None,
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

    /// A result the person has accepted, which is the only kind that may
    /// reach the spec.
    fn result_with(paths: &[&str]) -> ResultRecord {
        result_for("exec-1", ResultState::Kept, paths)
    }

    fn result_for(execution_id: &str, state: ResultState, paths: &[&str]) -> ResultRecord {
        ResultRecord {
            execution_id: execution_id.into(),
            outcome: ResultOutcomeKind::Finished,
            state,
            worktree_path: Some(format!("C:/wt/{execution_id}")),
            branch: Some(format!("agent-desk/{execution_id}")),
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
            changed_paths_unreadable: None,
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
            build_return_context(&s, &[result_with(&["a.rs"])], "exec-1", SpecReturnTarget::Tasks),
            Err(ReturnRefusal::NotFromASpec)
        );
    }

    #[test]
    fn work_that_has_not_finished_or_changed_nothing_is_refused_separately() {
        let s = task_session();
        assert_eq!(
            build_return_context(&s, &[], "exec-1", SpecReturnTarget::Tasks),
            Err(ReturnRefusal::StillWorking),
            "no result yet means the work is not settled"
        );
        assert_eq!(
            build_return_context(&s, &[result_with(&[])], "exec-1", SpecReturnTarget::Tasks),
            Err(ReturnRefusal::NothingChanged)
        );
        // Each refusal says what would change it, rather than just "no".
        assert!(ReturnRefusal::StillWorking.plain().contains("Wait for the work"));
        assert!(ReturnRefusal::NothingChanged.plain().contains("did not change any files"));
    }

    fn assistant_from(text: &str, execution_id: &str) -> SessionMessage {
        let mut m = assistant(text, MessageKind::Assistant);
        m.message_id = format!("m-{execution_id}-{text:.8}");
        m.execution_id = Some(execution_id.into());
        m
    }

    /// The defect this signature exists to prevent. A session with a lead and
    /// two helpers has one review panel per execution, so the person can be
    /// looking at helper A while the caller hands over whichever record
    /// happens to sit last in the file. The drafted spec update then names
    /// files they never saw -- plausible, because it is real work from the
    /// same session, just the wrong slice of it.
    #[test]
    fn the_draft_reports_on_the_execution_being_reviewed_not_the_last_one() {
        let s = task_session();
        let records = vec![
            result_for("helper-a", ResultState::Kept, &["src/parser.rs"]),
            result_for("helper-c", ResultState::Kept, &["src/unrelated.rs"]),
        ];

        let ctx = build_return_context(&s, &records, "helper-a", SpecReturnTarget::Tasks)
            .expect("the reviewed helper's result can report back");

        assert!(ctx.instruction.contains("src/parser.rs"), "{}", ctx.instruction);
        assert!(
            !ctx.instruction.contains("src/unrelated.rs"),
            "another execution's files reached the spec: {}",
            ctx.instruction
        );
    }

    /// Work the person threw away with Undo must never reach the spec. The
    /// rule existed only in the window, checked against a record the panel
    /// chose, while the backend acted on a different one -- so it was a rule
    /// about the spec that did not hold where the spec is reached.
    #[test]
    fn work_that_was_undone_cannot_be_told_to_the_spec() {
        let s = task_session();
        let records = vec![result_for("exec-1", ResultState::Discarded, &["src/parser.rs"])];

        assert_eq!(
            build_return_context(&s, &records, "exec-1", SpecReturnTarget::Tasks),
            Err(ReturnRefusal::WasThrownAway)
        );
    }

    /// Nor may work still under review: "Finished does not mean accepted".
    #[test]
    fn work_still_under_review_cannot_be_told_to_the_spec() {
        let s = task_session();
        for state in [ResultState::Reviewing, ResultState::RevisionRequested] {
            let records = vec![result_for("exec-1", state, &["src/parser.rs"])];
            assert_eq!(
                build_return_context(&s, &records, "exec-1", SpecReturnTarget::Tasks),
                Err(ReturnRefusal::StillBeingReviewed),
                "{state:?} is not an accepted result"
            );
        }
    }

    /// The file list and the account of the work have to come from the same
    /// execution. Unscoped, one execution's files were presented under
    /// another's prose -- two true halves assembled into an account that
    /// never happened, in text the person is invited to save into the spec.
    #[test]
    fn the_account_comes_from_the_same_execution_as_the_files() {
        let mut s = task_session();
        s.messages.push(assistant_from("Helper A rewrote the parser.", "helper-a"));
        s.messages.push(assistant_from("Helper C tidied the docs.", "helper-c"));

        let records = vec![result_for("helper-a", ResultState::Kept, &["src/parser.rs"])];
        let ctx = build_return_context(&s, &records, "helper-a", SpecReturnTarget::Tasks).unwrap();

        assert!(ctx.instruction.contains("Helper A rewrote the parser"), "{}", ctx.instruction);
        assert!(
            !ctx.instruction.contains("Helper C"),
            "another execution's words were presented as this one's: {}",
            ctx.instruction
        );
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
            &[result_with(&["src/parser.rs", "src/parser.test.rs"])],
            "exec-1",
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
            let ctx = build_return_context(&s, std::slice::from_ref(&r), "exec-1", target).expect("built");
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
            build_return_context(&s, &[result_with(&["a.rs"])], "exec-1", SpecReturnTarget::Tasks).unwrap();
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
            build_return_context(&s, &[result_with(&["a.rs"])], "exec-1", SpecReturnTarget::Proposal).unwrap();
        assert_eq!(ctx.change_id, "add-the-thing");
        assert!(ctx.instruction.contains("- a.rs"));
    }
}
