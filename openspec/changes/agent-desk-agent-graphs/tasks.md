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
      PARTIAL, closer than it looks: the mechanism itself is real and tested at the unit level
      (see `agent-desk-conversation-shell` tasks.md 6.1, reversed this pass) --
      `started_for_execution` returns `false` for a Plan session until `graph_started_at` is
      set by `start_graph_at`'s own Start handler (`agent_graph.rs:986`, "never cleared once
      set -- Start is a one-way transition"), and `false` reaches a hard `--deny-tool=write`
      at process launch. What is missing is the specific test this task asks for: an actual
      attempted-write-during-a-Plan-proposal-turn assertion against a dirty checkout. No such
      test exists under this name or shape. Left unchecked for that missing test.

## 3. Helper runtime

- [x] 3.1 Create a unique execution ID, branch, and marked worktree for each helper.
- [x] 3.2 Give helpers bounded prompt/context, path allowance, turn budget, and done check.
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
      Confirmed NOT DONE: grepped `commands/agent_graph.rs` for `operation_id`/`OperationId`/
      a queue type and found none. Retries today happen only because `integrate_helper_into`
      is idempotent and re-runs whole on the next completion event -- there is no durable,
      ID-addressed operation queue or explicit completion ordering.
- [x] 5.2 Provision a dedicated lead integration worktree from the recorded base before any
      helper starts. Never use `session.header.repo_path` or another user-open checkout as
      the integration target.
      `ensure_integration_worktree` (`agent_graph.rs:1190`) provisions a dedicated worktree
      from `HEAD` via `worktree::add`/`mark_as_run_worktree`, persists its path on the lead's
      own `ExecutionRecord`, and is proven by
      `integration_never_writes_into_the_sessions_repo_path`, which sets `header.repo_path`
      to a stand-in "user's open checkout" containing a same-named file and asserts it is
      byte-untouched after integration while the dedicated worktree receives the write.
- [x] 5.3 Compute each helper's real delta from its worktree and recorded base, including
      staged, unstaged, and committed changes. Do not require or simulate a helper commit.
      `helper_delta` (`agent_graph.rs:1413`) uses `diff_tree_to_workdir_with_index`, proven by
      four direct tests: `helper_delta_sees_an_unstaged_uncommitted_edit`,
      `_sees_a_staged_uncommitted_edit`, `_sees_committed_work_too`, and
      `_reports_nothing_when_workdir_equals_base`. This corrects the file's own prior
      "SECOND AUDIT 2026-08-22" note, which claimed the delta was base-tree-to-HEAD only --
      that note is stale against the code in the same commit (`21bdb3e`) that added it.
- [x] 5.4 Apply repository operations without lossy text conversion: add, modify, delete,
      rename, binary bytes, symlink target/type, executable bit, and file mode. Distinguish a
      missing path from an empty file and turn read/write failures into typed paused states.
      `read_content_at`/`apply_operation`/`write_content_atomic` (`agent_graph.rs:1285,1500,1558`)
      read blobs as raw bytes (never UTF-8-decoded when `blob.is_binary()`), handle
      `Add/Modify/Delete/Rename` with real content, and set the executable bit and create
      symlinks natively. Tests cover delete, rename, binary bytes, and executable-bit change
      (`apply_operation_delete_removes_the_file`,
      `helper_delta_represents_binary_content_as_raw_bytes_never_utf8_decoded`, etc.) and
      write failures return a typed `ApplyOperationOutcome::Failed`, never a silent success.
      Symlink target/type has code (`create_symlink`, `read_symlink_target`) but no dedicated
      test -- noted under 5.10, not blocking this task's core claim.
- [ ] 5.5 Make one integration operation atomic or durably resumable. A mid-operation failure
      must not produce a partial change that is later reported as integrated or Finished.
      PARTIAL: per-file writes are atomic (temp-write-then-rename in `write_content_atomic`),
      and `integrate_helper_into` is designed to be idempotently re-run in full on the next
      completion event (it compares `helper_text == integrated_text` before writing, so a
      re-run skips already-applied operations) -- a real, reasoned resumability story, and a
      failed operation is reported on the helper's own `output_summary` without flipping its
      state to a false "Finished/integrated". But there is no test that actually simulates a
      crash mid-batch (kill after operation 2 of 5, restart, verify completion) -- the
      resumability claim rests on code inspection, not a proof. Left unchecked pending that
      test, called for explicitly in 5.10.
- [x] 5.6 Detect conflicts against the live integration worktree and preserve base, helper,
      and integrated versions plus operation metadata. Conflict refresh/restart must not
      reclassify the node as Interrupted.
      `detect_conflict`/`IntegrationConflict` preserve all three texts (proven by
      `record_conflict_then_resolve_keep_helper_preserves_both_texts_until_resolved`), and
      `reconcile_executions` (`session_recovery.rs:121`) has an explicit
      `if execution.conflict.is_some() { continue; }` carve-out with its own test,
      `a_conflicted_helper_survives_reconciliation_without_becoming_interrupted`. This
      corrects the file's own prior finding (from the 2026-08-20/21 passes and repeated in
      6.3 below) that a conflicted helper is wrongly reclassified `Interrupted` on reload --
      that bug is fixed as of `41a1fe6`, with a test naming the exact scenario.
- [x] 5.7 Apply Keep Helper, Keep Integrated, or merged content to the integration worktree,
      then resume only that operation and leave peer helpers untouched.
      `resolve_conflict_at` (`agent_graph.rs:2552`) writes the chosen text via
      `write_content_atomic` into the integration worktree specifically (proven by
      `resolving_a_conflict_writes_into_the_integration_worktree_not_the_users_checkout`),
      supports `KeepHelper`/`KeepIntegrated`/`UseMerged`, and
      `resolving_one_nodes_conflict_never_touches_a_sibling_node` proves isolation.
- [x] 5.8 Build helper-scoped results from helper worktrees and one primary graph result from
      the lead integration worktree. The combined result must link back to every helper.
      `finish_graph_with_combined_result` (`agent_graph.rs:2315`) builds one `ResultRecord`
      keyed to the lead's execution ID from the integration worktree and calls
      `link_helper_results` with every helper's execution ID, proven by
      `finish_graph_with_combined_result_builds_the_result_and_links_every_helper`.
- [x] 5.9 Run a real lead combined-review/check turn after integration. Only a successful,
      persisted combined result may move the graph to Finished; a node counter is not review.
      `start_or_check_lead_review`/`launch_lead_review` (`agent_graph.rs:1964,2143`) launch a
      genuine `CliAgent::discover` + `cli_run::run_task` process turn (identical launch
      pattern to solo/helper executions) with a prompt summarizing every helper's report and
      asking the lead to review, check, and fix the combined tree. The graph only reaches
      `Finished` once this review execution itself reaches a terminal state
      (`ReviewFinishedBuildResultAndFinish`), proven by four state-machine tests including
      `start_or_check_lead_review_does_nothing_while_a_helper_is_still_active` (graph stays
      `Working`, not flipped by a node count). This directly contradicts the file's own
      "SECOND AUDIT 2026-08-22" summary, which is stale against the code landed in the same
      commit.
- [ ] 5.10 Add real-repository tests for uncommitted edits, staged edits, delete, rename,
      binary content, symlink/mode where supported, same-line conflict, restart during
      integration, and proof that the user's checkout remains byte-identical.
      PARTIAL: uncommitted/staged/committed edits, delete, rename, binary content,
      executable-bit, same-line conflict, and byte-identical-checkout are all covered by real
      tests (see 5.3/5.4/5.6 evidence above). Missing: a symlink-specific integration test
      (code exists, untested) and a restart-during-integration test (kill mid-batch, restart,
      verify exactly-once completion) -- left unchecked for those two gaps specifically.

## 6. UI and recovery

- [x] 6.1 Project graph nodes from backend records; no separate frontend graph truth.
- [ ] 6.2 Show node state, role/model, current action, dependency, files, and output link.
      PARTIAL: `AgentGraphPanel.tsx` genuinely renders state (`nodeStatusLabel`, live status
      word + dot), role (`execution.helperRole`), dependency (`blockedOn`/`waitingForSlot`
      from `buildGraphTree`), and files (`changedFileCount`). Missing: no "current action"
      field anywhere (only the coarse status word -- no "reading file X"/step-level text), no
      model display, and the "output link" is explicitly a disabled button --
      `<button disabled title="A helper's own conversation cannot be opened yet">Open
      conversation</button>` (line ~226-233). Three of six sub-fields are not shown.
- [x] 6.3 Reconstruct running/waiting/conflicted graph after window/app restart.
      Reversed from the 2026-08-20/21/22 passes' repeated finding: `reconcile_executions`
      (`session_recovery.rs:112`) now has an explicit `if execution.conflict.is_some() {
      continue; }` carve-out (added in `41a1fe6`, same session as the prior finding), proven
      by `a_conflicted_helper_survives_reconciliation_without_becoming_interrupted`, which
      asserts a conflicted node's state stays `NeedsInput` (not `Interrupted`) and both
      preserved texts survive the reload. Running/waiting executions genuinely alive in the
      registry are left alone; genuinely-dead ones are correctly marked `Interrupted`
      (`reconcile_session_executions`, called on every `agent_session_get`). All three
      required states -- running, waiting, conflicted -- are now provably reconstructed.
- [ ] 6.4 Recover orphaned worktrees and never delete the only copy of work.
      PARTIAL: real, wired orphan-worktree recovery exists --
      `agent_result_find_orphaned`/`agent_result_find_orphaned_all` (`agent_result.rs:1174`)
      is called from `useOrphanResultReconciliation.ts`, itself used in `AgentDeskView.tsx`
      (confirmed live, not dead code), and only removes a worktree once its `ResultRecord`
      reaches `Committed`/`Discarded` (`DirtyChoice::Refuse` elsewhere in the same file
      protects dirty trees). But this is scoped to `ResultRecord` worktrees (solo results and
      the one combined graph result once built) -- it does not cover a helper worktree that
      crashes or is abandoned before any result/conflict is ever recorded on it. Left
      unchecked because that specific graph-helper-worktree gap is real, not because nothing
      was built.
- [ ] 6.5 Test two independent edits, stopped peer, simultaneous gates, conflict, crash.
      NATIVE: requires a running multi-agent app session to exercise real concurrent
      scenarios (simultaneous gates, a live crash) -- no automated harness for this exists or
      reasonably could without standing up real provider processes. Left unchecked.
- [ ] 6.6 Record Gate 5 evidence.
      NATIVE: an evidence-gate record from an actual run; nothing to verify in code.

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
