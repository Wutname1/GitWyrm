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
- [ ] 2.5 Run the lead's Plan proposal with `started=false` and provider-level writes denied.
      Start is the only transition that may provision write-capable graph worktrees or launch
      helpers. Test the proposal turn against an attempted write and a dirty checkout.

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

**SECOND AUDIT 2026-08-22 - supersedes the 2026-08-21 no-write finding:** helper files are
now copied, but the implementation still cannot produce a safe combined result. It targets
the user's open checkout, derives changes only from base-tree-to-HEAD (while helpers normally
leave uncommitted work), and converts missing/binary/read-error paths to empty UTF-8 text.
The following tasks define the actual release boundary.

- [ ] 5.1 Add a serialized integration queue with durable operation IDs and deterministic
      completion order. A restart must resume the next unapplied operation exactly once.
- [ ] 5.2 Provision a dedicated lead integration worktree from the recorded base before any
      helper starts. Never use `session.header.repo_path` or another user-open checkout as
      the integration target.
- [ ] 5.3 Compute each helper's real delta from its worktree and recorded base, including
      staged, unstaged, and committed changes. Do not require or simulate a helper commit.
- [ ] 5.4 Apply repository operations without lossy text conversion: add, modify, delete,
      rename, binary bytes, symlink target/type, executable bit, and file mode. Distinguish a
      missing path from an empty file and turn read/write failures into typed paused states.
- [ ] 5.5 Make one integration operation atomic or durably resumable. A mid-operation failure
      must not produce a partial change that is later reported as integrated or Finished.
- [ ] 5.6 Detect conflicts against the live integration worktree and preserve base, helper,
      and integrated versions plus operation metadata. Conflict refresh/restart must not
      reclassify the node as Interrupted.
- [ ] 5.7 Apply Keep Helper, Keep Integrated, or merged content to the integration worktree,
      then resume only that operation and leave peer helpers untouched.
- [ ] 5.8 Build helper-scoped results from helper worktrees and one primary graph result from
      the lead integration worktree. The combined result must link back to every helper.
- [ ] 5.9 Run a real lead combined-review/check turn after integration. Only a successful,
      persisted combined result may move the graph to Finished; a node counter is not review.
- [ ] 5.10 Add real-repository tests for uncommitted edits, staged edits, delete, rename,
      binary content, symlink/mode where supported, same-line conflict, restart during
      integration, and proof that the user's checkout remains byte-identical.

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

## Status 2026-08-22 second audit

The integration implementation changed after the prior audits: clean helper files are now
copied, so the old statement that integration performs no writes is historical. The new path
is still release-blocking because it writes to the user's checkout, observes committed HEAD
instead of the helper's ordinary uncommitted work, and performs lossy text-only copies.
Tasks 2.5 and 5.1-5.10 above supersede the old integration assessment and define completion.

## Prior status 2026-08-20 (historical)

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

## Prior status 2026-08-21 (historical)

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

**Historical finding, fixed incompletely after this pass -- integration did not apply anything.**
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
