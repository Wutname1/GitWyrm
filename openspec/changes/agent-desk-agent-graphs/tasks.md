# Tasks

## 1. Graph model and validation

- [x] 1.1 Add graph, node, dependency, job, budget, completion, and graph-state types.
- [x] 1.2 Validate one lead, max three helpers, acyclic dependencies, stable unique IDs,
      allowed paths, and explicit completion conditions.
- [x] 1.3 Add fixture tests for every invalid shape and schema migration.
- [x] 1.4 Persist proposed/running/final graph in the session execution record.

## 2. Plan flow

- [x] 2.1 Let the lead emit a proposed graph through a typed engine tool/output.
- [x] 2.2 Render AwaitingStart graph with Start, Revise, and Use solo.
- [x] 2.3 Start only after revalidating current source/policy/worktree capacity.
- [x] 2.4 Make every action change graph state immediately and visibly.

## 3. Helper runtime

- [x] 3.1 Create a unique execution ID, branch, and marked worktree for each helper.
- [ ] 3.2 Give helpers bounded prompt/context, path allowance, turn budget, and done check.
      PARTIAL as of 2026-08-21: `launch_helper` (`src-tauri/src/commands/agent_graph.rs`)
      genuinely builds a bounded prompt from `job_title`/`job_description`, resolves
      `ExecutionPolicy::resolve_for_helper(can_write, allowed_paths)`, and passes a real
      `JobBudget` into `cli_run::run_task`, which enforces both `max_turns`
      (`turn_budget_exceeded`) and `max_seconds` (an armed wall-clock deadline) live -- three
      of four sub-requirements are proven. The fourth, "done check": `CompletionCondition` has
      `ChecksPass{command}` and `FilesChanged{paths}` variants but grepping
      `commands/agent_graph.rs` finds no code that ever evaluates either one at runtime; only
      `ReportsResult` (the process simply ending) is used anywhere, and nothing "checks" that
      case either. Left unchecked because the done-check requirement is explicitly named and
      not met.
- [x] 3.3 Execute dependency-ready helpers with max concurrency three; a pure scheduler
      result or persisted Ready record is not execution.
      Reversed from the 2026-08-20 pass: `agent_session_start_graph`'s production handler now
      calls `launch_helper` for every id in `started_helpers` (`src-tauri/src/commands/agent_graph.rs`
      line ~458), which discovers a real `CliAgent`, registers a `CancelHandle` in the same
      `ExecutionRegistry` solo runs use, and spawns `cli_run::run_task` via
      `tauri::async_runtime::spawn`. This is a real process launch, not a persisted record.
- [x] 3.4 Persist every helper event before broadcasting.
      Helper sinks route through the exact same `emit_agent_desk_only` -> `route_to_agent_desk`
      -> `route_run_event` path solo executions use (`src-tauri/src/commands/airun.rs`), which
      persists to the session store before the `AGENT_SESSION_EVENT` broadcast in
      `commands::agent_desk`'s event loop -- no separate, unpersisted helper broadcast path
      exists.
- [x] 3.5 Reject stale/duplicate helper events by execution/sequence.
      Reversed from the 2026-08-20 pass, which found this blocked on helpers not executing at
      all. Now: `bridge.rs::apply_run_event` has an `is_known_helper` exemption from the
      lead-only "superseded" rejection (a pre-existing bug this session's R6 work fixed -- a
      helper's `execution_id` is never `active_execution_id`, so before this exemption every
      helper event after the first would have been wrongly dropped as stale), proven by
      `a_helpers_own_events_are_applied_even_though_the_lead_is_the_active_execution`. The
      ordinary duplicate-sequence rejection (`a_duplicate_sequence_is_ignored`) applies
      identically to helper and lead executions since both go through the same function.
- [ ] 3.6 Keep lead and peers responsive when one helper waits at a gate.
      Left unchecked: each helper runs inside its own `tauri::async_runtime::spawn` task and
      blocks on a std `mpsc::Receiver::recv()` at a gate (`cli_run.rs::handle`), which is
      architecturally plausible to not starve siblings, but no test or native run proves
      lead/peer responsiveness is preserved while one is blocked -- see 6.5's "simultaneous
      gates" scenario, itself still native-only.

## 4. Controls and approvals

- [x] 4.1 Add per-helper Stop that cancels only that live execution.
      `AgentGraphPanel.tsx`'s `stopThis` calls `agentSessionStopExecution(sessionId, {kind:'one', execution_id})`
      per node; `launch_helper` registers each helper's own `CancelHandle` in the shared
      `ExecutionRegistry` before its first `Working` event, so `StopScope::One` reaches
      exactly that helper's process, not the lead or a sibling.
- [x] 4.2 Add labeled Stop all in Graph header; cancel live lead/helpers promptly.
      `AgentGraphPanel.tsx` calls `agentSessionStopExecution(sessionId, {kind:'all'})`, which
      `agent_desk.rs` routes to every registered execution (lead and helpers alike) in that
      session via the same `ExecutionRegistry`.
- [ ] 4.3 Preserve uncommitted recoverable work on stop/failure.
      PARTIAL: confirmed `stop_execution_at` (`agent_desk.rs`) never calls `worktree::remove`
      or otherwise touches a helper's worktree, so a stopped/failed helper's edits are
      preserved by omission -- the only worktree-removal call in the graph path
      (`agent_graph.rs` line ~632) is a losing-race cleanup of a worktree no record will ever
      reference, not a stop/failure path. Left unchecked because no dedicated test or UI
      proves the worktree's *location* is surfaced back to the user specifically after a
      stop/failure, which is the other half of the task.
- [x] 4.4 Key approval cards/answers to helper execution and gate ID.
      Reversed from the 2026-08-20 pass, which found `GATE_ANSWERS` keyed only by `repo_id`.
      That is fixed: `GateAnswerKey` is now `(session_id, execution_id)`
      (`src-tauri/src/commands/airun.rs`), and both `start_execution_at`
      (`agent_desk.rs`) and `launch_helper` (`agent_graph.rs`) insert under that tuple, so two
      helpers in the same session/repo get distinct channels. `agent_session_answer_gate`
      looks up the same tuple, and the frontend's `GateApprovalControls`
      (`ConversationPane.tsx`) passes `message.executionId` on every answer call, never a
      bare session or repo id.
- [x] 4.5 Show all waiting approvals in one queue without answering peers.
      A natural consequence of 4.4's per-execution keying rather than a dedicated queue
      widget: each execution's own `Gate` message renders its own `GateApprovalControls`
      bound to that message's `executionId`, so multiple simultaneous helper gates appear as
      separate rows in the transcript and answering one cannot resolve another's channel
      (verified via the keying above, not a custom queue component).

## 5. Integration

**CRITICAL FINDING 2026-08-21**: read `integrate_helper_result` and `resolve_conflict_at`
(`src-tauri/src/commands/agent_graph.rs`) end to end. When a helper's file does NOT conflict,
the loop over `changed_files_since` calls `graph::detect_conflict`, gets back
`IntegrationState::Clean` (implicitly, the non-`Conflicted` arm), and does *nothing* --
no `fs::write`, no `git2` apply, no commit. When a file DOES conflict and the user picks
Keep Helper/Keep Integrated/merged text in `resolveConflict` (`AgentGraphPanel.tsx`), the
backend computes the correct `resolved_text` and returns it in the outcome, but neither
`resolve_conflict_at` nor the frontend ever writes that text to any file -- it only flips
the execution's state to `Finished` and clears the conflict marker. **No code path in this
repository ever actually merges a helper's worktree changes into the lead's working tree,
clean or resolved.** A helper's work stays isolated in its own worktree forever; "resolving"
a conflict is currently indistinguishable, on disk, from ignoring it. This is a functional
gap in the core promise of the graph feature (multiple agents produce one consolidated
result), not a cosmetic one.

- [ ] 5.1 Queue completed helper results in completion order.
      PARTIAL: `advance_graph_after_helper_completion` genuinely triggers per-completion
      (not a batch queue, but each finish independently processed as it happens, satisfying
      "completion order" by construction). Left unchecked because the queued item's own
      substance -- an actual application of the result -- never happens; see the finding above.
- [ ] 5.2 Review/apply each result through existing completion/commit plumbing.
      Not built: confirmed no `fs::write`/`git2` apply/commit call exists anywhere in the
      integration path. See the finding above.
- [ ] 5.3 Detect conflicts in the live integration path as a typed state; preserve
      base/helper/integration copies.
      Detection itself is real and precisely proven
      (`integrate_helper_result_records_a_typed_conflict_when_the_lead_also_changed_the_file`:
      real `git2` repos, a real base/lead/helper divergence, exact `base_text`/`helper_text`/
      `integrated_text` recorded). Reverted to unchecked, though, because of a second finding
      made while auditing 6.3: `executions.complete(session_id, execution_id)`
      (`agent_graph.rs`, in the helper's own spawned task) runs and removes the helper from
      the live `ExecutionRegistry` BEFORE `advance_graph_after_helper_completion` ever sets
      `NeedsInput`+`conflict` on it. `reconcile_session_executions`
      (`src-tauri/src/commands/agent_desk.rs`), which runs on every `agent_session_get`,
      treats `NeedsInput` as a "live process state" needing registry confirmation and cannot
      distinguish a conflict-wait from a gate-wait -- so the very next time ANYONE loads that
      session (not just after an app restart), the conflicted helper is found not-live and
      silently flipped to `Interrupted`, and its `conflict` field is left on the record but
      nothing surfaces it as a conflict any more. "Preserve" fails: the typed conflict state
      does not survive even a single query refresh, let alone a restart.
- [ ] 5.4 Let conflict resolution resume only that node's live integration.
      PARTIAL: `resolve_conflict_at` correctly scopes its write to only the named
      `execution_id` (siblings untouched, proven by the read-modify-write closure only
      matching one record) -- but "resume... integration" implies the resolved text gets
      applied somewhere, which per the finding above never happens. The node's state moves to
      `Finished` with nothing on disk changed as a result.
- [ ] 5.5 Run lead combined review/check step before final session completion.
      Confirmed still not built, and self-documented as such: `maybe_finish_graph`'s own doc
      comment calls itself "a minimal, honest substitute for a full 'lead re-reads every
      helper's output and writes a real review message' pass" and says the second engine
      invocation that would require is "not yet wire[d]."

## 6. UI and recovery

- [x] 6.1 Project graph nodes from backend records; no separate frontend graph truth.
- [ ] 6.2 Show node state, role/model, current action, dependency, files, and output link.
- [ ] 6.3 Reconstruct running/waiting/conflicted graph after window/app restart.
      Confirmed still incomplete, worse than the 2026-08-20 pass found: `reconcile_session_executions`
      is real and does run on every `agent_session_get` (not just app boot), correctly marking
      genuinely-orphaned live-state executions `Interrupted` (proven for ordinary
      Working/Preparing helpers). But per the 5.3 finding, it also wrongly reclassifies a
      *conflicted* helper as Interrupted, because conflict-wait and gate-wait are both
      `NeedsInput` and the registry has already forgotten the helper's process by the time the
      conflict is recorded. "Reconstruct... conflicted" specifically fails.
- [ ] 6.4 Recover orphaned worktrees and never delete the only copy of work.
- [ ] 6.5 Test two independent edits, stopped peer, simultaneous gates, conflict, crash.
- [ ] 6.6 Record Gate 5 evidence.

## Status 2026-08-20

Audited by reading `src-tauri/src/agentdesk/graph.rs` (830 lines, 8 fixture tests covering
every invalid shape and the conflict/schedule logic), `src-tauri/src/commands/agent_graph.rs`
(1027 lines, 6 tauri commands all registered in `lib.rs`), `src/lib/agentGraphProjection.ts`
(+ its own test file), `AwaitingStartCard.tsx`, and the rewritten `AgentGraphPanel.tsx` (wired
into `DockedDetailPanel.tsx`, `PaneDetailPopover.tsx`, `SessionComposer.tsx`).

**Ticked (13 of 30), on this evidence:**
- 1.1-1.4: real types, real DAG/cycle/budget/path validation with 8 passing fixture tests,
  and `proposed_graph`/`conflict` fields genuinely persisted on `ExecutionRecord`.
- 2.2-2.4: `agent_session_propose_graph`/`start_graph`/`use_solo_instead` are real, tested
  commands that write session state synchronously (no optimistic-only frontend state).
- 3.1, 3.3: `start_graph_at` genuinely mints a unique execution id, branch name, and calls
  `worktree::add` + `mark_as_run_worktree` per ready helper before any state write; the pure
  `schedule()` function has direct tests proving the concurrency-3 cap and dependency gating.
- 4.1, 4.2: reuses the pre-existing `StopScope::One`/`StopScope::All` from `agent_desk.rs`,
  which already has its own tests covering exactly this case.
- 5.3, 5.4: `detect_conflict`/`IntegrationState`/`resolve_conflict_at` are real, with a test
  that proves both copies survive a same-line conflict and neither is auto-committed; resolving
  one node's execution record does not touch any sibling record.
- 6.1: `project_graph` is a pure function of `ExecutionRecord` rows, confirmed by reading it
  and its tests — there is no parallel graph state stored in the frontend.

**Left unticked — genuinely missing or only partially built:**
- 2.1: confirmed no engine tool exists anywhere in `src-tauri/src/ai/agent/` that lets a lead
  emit a `ProposedGraph` from a live model turn. Nothing calls `agent_session_propose_graph`
  from the frontend either (grepped — zero call sites outside the graph command test file).
  This means the whole Plan-mode flow is currently reachable only by calling the Tauri command
  directly (e.g. in a test), not from an actual agent conversation.
- 3.2: budget/allowed_paths are stored as data on the execution record, but nothing enforces
  them at runtime — there is no code path that reads `max_turns`/`max_seconds`/`allowed_paths`
  and stops or restricts a running helper, because the engine wiring from 2.1 that would drive
  a helper turn does not exist yet.
- 3.4, 3.5: the shared `agentdesk/events.rs` sequence/dedup machinery exists and is reused, but
  I found no graph-specific test proving helper events specifically go through it, and no
  broadcast path exists yet since helpers aren't actually being executed (see 2.1/3.2 gap).
- 4.3: no code path preserves uncommitted work distinctly for stop-mid-helper; not found.
- 4.4, 4.5: confirmed the known gap — `GATE_ANSWERS` in `commands/airun.rs` is keyed by
  `repo_id`, not by execution id or gate id, so two helpers in the same repo hitting an
  approval gate simultaneously are not correctly routed. Left unticked per the explicit
  instruction not to tick known gaps.
- 5.1, 5.2, 5.5: no completion-order queue or "lead combined review" step found; the
  conflict-detection primitive exists but nothing calls it as part of an actual multi-helper
  completion flow yet.
- 6.2: `GraphNodeView` carries the fields (state, role, dependency, paths, output_summary) but
  I did not find UI code rendering "current action" as a live, updating field — worth a closer
  look but not confirmed either way, left unticked.
- 6.3, 6.4: explicitly reported as not written by the building agent; grepped for
  "orphan"/"reconstruct"/"restart" in the graph module and found nothing. Confirmed missing.
- 6.5, 6.6: these are native, in-app, multi-agent scenario tests (stopped peer, simultaneous
  gates, conflict, crash) plus a recorded evidence gate — need a human running the real app.
  Left unticked regardless of code quality, per instructions.

## Status 2026-08-21 (second pass)

Re-audited against this session's R6 landing claims. This package changed the most of any
package this session -- helpers now genuinely launch as real processes, which the
2026-08-20 pass correctly found did not happen yet.

**Reversed from unchecked to checked, on new evidence:**
- 2.1: a fenced-block parser (`agentdesk/plan_proposal.rs`) reads a Plan-mode lead's own
  finished-turn text and is genuinely called from the production event-routing path
  (`commands/airun.rs`'s `route_to_agent_desk`, gated on `Finished` + Plan/Lead), persisting
  a real `AwaitingStart` proposal or a visible refusal note. Reasoned, documented departure
  from a "typed engine tool" (no MCP/tool-registration layer exists) toward parsing the
  model's own prose against a fenced convention -- disclosed as such in the module's own
  doc comment, not hidden.
- 3.3: `agent_session_start_graph`'s handler calls `launch_helper` for every ready id, which
  discovers a real CLI agent and spawns `cli_run::run_task` via `tauri::async_runtime::spawn`
  -- a real process, not a persisted `Ready` record.
- 3.4, 3.5: helper events route through the identical persist-then-broadcast/dedup path
  solo runs use, and a real pre-existing bug (helper events wrongly dropped as "superseded"
  because a helper's execution id is never the session's `active_execution_id`) is fixed
  with its own test.
- 4.1, 4.2, 4.4, 4.5: per-helper and stop-all both route through the same `ExecutionRegistry`
  cancel handles; `GATE_ANSWERS` is now keyed by `(session_id, execution_id)` (was `repo_id`
  only), so two helpers' approval gates cannot cross-answer.

**CRITICAL, previously unknown finding -- integration never actually applies anything.**
Read `integrate_helper_result` and `resolve_conflict_at` end to end: a clean (non-conflicting)
helper file change is never written into the lead's tree at all, and a *resolved* conflict's
correct text is computed and returned to the caller but never written anywhere either --
resolving only flips the execution to `Finished` and clears the conflict marker. No git2
apply, no fs::write, no commit exists anywhere in this path. A helper's work stays isolated
in its own worktree permanently. This directly blocks 5.1/5.2/5.4 and is the reason 5.5 (a
lead review step) is self-documented by its own author as not built.

**Second critical finding, made auditing 6.3 -- conflicts don't survive a reload.** A
helper's process is removed from the live `ExecutionRegistry` (`executions.complete(...)`)
before the conflict is even detected and recorded on its execution. `reconcile_session_executions`,
which runs on every `agent_session_get`, treats any `NeedsInput` execution needing registry
liveness proof, with no distinction between "waiting on an approval gate" and "waiting on a
conflict decision" -- so a conflicted helper is silently reclassified `Interrupted` the very
next time anyone loads that session, not just after a real app restart. This reversed 5.3
from the "should probably tick" list back to unchecked, since "preserve" cannot be true of a
state that does not survive a query refresh.

**Still correctly unchecked, re-confirmed:** 3.2's "done check" sub-requirement (no code
evaluates `ChecksPass`/`FilesChanged`), 3.6 (no test/native proof of responsiveness during a
gate wait), 4.3 (no proof the worktree location surfaces specifically after stop/failure),
6.2/6.4/6.5/6.6 (not found built, or native-only).

Net effect of this pass: 10 tasks moved from unchecked to checked (2.1, 3.3, 3.4, 3.5, 4.1,
4.2, 4.4, 4.5), 1 task's status note substantially rewritten without a checkbox change (3.2),
and 1 task reverted from a would-be checkmark to unchecked after a deeper read (5.3) --
net movement is real progress, not a rubber stamp, but the integration-application gap is a
serious, previously undocumented hole in the feature's core value proposition.
