# Tasks

## 1. Graph model and validation

- [x] 1.1 Add graph, node, dependency, job, budget, completion, and graph-state types.
- [x] 1.2 Validate one lead, max three helpers, acyclic dependencies, stable unique IDs,
      allowed paths, and explicit completion conditions.
- [x] 1.3 Add fixture tests for every invalid shape and schema migration.
- [x] 1.4 Persist proposed/running/final graph in the session execution record.

## 2. Plan flow

- [ ] 2.1 Let the lead emit a proposed graph through a typed engine tool/output.
- [x] 2.2 Render AwaitingStart graph with Start, Revise, and Use solo.
- [x] 2.3 Start only after revalidating current source/policy/worktree capacity.
- [x] 2.4 Make every action change graph state immediately and visibly.

## 3. Helper runtime

- [x] 3.1 Create a unique execution ID, branch, and marked worktree for each helper.
- [ ] 3.2 Give helpers bounded prompt/context, path allowance, turn budget, and done check.
- [x] 3.3 Schedule dependency-ready helpers with max concurrency three.
- [ ] 3.4 Persist every helper event before broadcasting.
- [ ] 3.5 Reject stale/duplicate helper events by execution/sequence.
- [ ] 3.6 Keep lead and peers responsive when one helper waits at a gate.

## 4. Controls and approvals

- [x] 4.1 Add per-helper Stop that cancels only that execution.
- [x] 4.2 Add labeled Stop all in Graph header; cancel lead/helpers promptly.
- [ ] 4.3 Preserve uncommitted recoverable work on stop/failure.
- [ ] 4.4 Key approval cards/answers to helper execution and gate ID.
- [ ] 4.5 Show all waiting approvals in one queue without answering peers.

## 5. Integration

- [ ] 5.1 Queue completed helper results in completion order.
- [ ] 5.2 Review/apply each result through existing completion/commit plumbing.
- [x] 5.3 Detect conflicts as a typed state; preserve base/helper/integration copies.
- [x] 5.4 Let conflict resolution resume only that node's integration.
- [ ] 5.5 Run lead combined review/check step before final session completion.

## 6. UI and recovery

- [x] 6.1 Project graph nodes from backend records; no separate frontend graph truth.
- [ ] 6.2 Show node state, role/model, current action, dependency, files, and output link.
- [ ] 6.3 Reconstruct running/waiting/conflicted graph after window/app restart.
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
