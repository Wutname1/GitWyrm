# Agent Desk implementation reset

Recorded 2026-08-21 after an end-to-end audit of the working branch. This is the controlling
handoff until every Reset gate passes. It does not authorize product work outside these
steps, and it does not treat existing scaffolding as shipped behavior.

## Current truth

The branch contains useful session storage, models, adapters, panels, and tests, but it is
not a working Agent Desk release. The most serious gaps are behavioral:

- Stop updates persisted state but does not cancel a real CLI execution.
- Ask, Plan, Review, and Summarize do not enforce read-only authority at the engine boundary.
- source actions create a session but do not start the requested agent operation;
- a message appended during a run is not delivered to that running agent;
- graph helpers are provisioned as records/worktrees but are not launched;
- results and review UI are not created/mounted from live completion;
- OpenSpec context and accepted task completion are not connected to the live run;
- Import UI exists but is not reachable;
- Match selected apps writes a broad batch without the promised item/destination preview;
- repository filtering prevents a truly app-wide, cross-project session workspace; and
- the Rust suite and strict OpenSpec validation are currently red.

## Completion vocabulary

Every task and review must use these terms consistently:

1. **Defined** - the type/schema/interface exists.
2. **Implemented in isolation** - a helper, command, or component works under direct tests.
3. **Wired** - the production entry point calls it and its result reaches the real UI.
4. **Proven** - success, refusal, failure, cancellation, restart, and visible feedback have
   named automated or native evidence.
5. **Complete** - wired and proven. Only this state receives an OpenSpec checkmark.

## R0 - freeze and restore a trustworthy baseline

- [ ] R0.1 Stop concurrent edits or record the owner and purpose of every dirty file.
- [ ] R0.2 Reconcile the three existing dirty files without overwriting another agent's work.
- [ ] R0.3 Fix strict validation for `add-ai-agent-engine`; every normative requirement uses
      SHALL or MUST and still expresses the intended default-provider behavior.
- [ ] R0.4 Fix the failing agent-config location test without weakening client detection.
- [ ] R0.5 Run one Cargo test process only; record exact pass/fail/ignored counts.
- [ ] R0.6 Run TypeScript typecheck and the complete frontend unit suite.
- [ ] R0.7 Record `git status --short`, test commands, and outputs in a Gate 0 evidence file.

**R0 gate:** clean ownership, strict OpenSpec green, TypeScript green, frontend tests green,
Rust tests green. No product behavior is claimed yet.

## R1 - make execution authority real

- [ ] R1.1 Replace ignored mode/team/provider parameters with an `ExecutionPolicy` constructed
      once from session intent, operating mode, team mode, and provider override.
- [ ] R1.2 Pass the policy into provider discovery and tool dispatch; do not rely on UI state.
- [ ] R1.3 Deny file writes, patching, worktree creation, commits, pushes, and host writes for
      Ask, Review, and Summarize.
- [ ] R1.4 Allow Plan to read and propose a structured plan, but deny execution until Start.
- [ ] R1.5 Keep Auto subject to destructive-action, secret, push, and host-write approvals.
- [ ] R1.6 Make unsupported provider overrides fail visibly instead of silently using Copilot.
- [ ] R1.7 Add adversarial live-adapter tests where the model asks to write under every
      read-only intent and the engine refuses before touching disk.
- [ ] R1.8 Native-test Review/Summarize against a dirty checkout and prove byte-identical files.

**R1 gate:** read-only behavior is enforced below the prompt layer. A malicious or confused
provider cannot write around it.

## R2 - own and cancel every live execution

- [ ] R2.1 Add a runtime registry keyed by durable session and execution ID, not repository ID.
- [ ] R2.2 Store cancellation token/process shutdown ownership before emitting Working.
- [ ] R2.3 Route Stop one to exactly one registered execution.
- [ ] R2.4 Route Stop all to lead and every helper in that session only.
- [ ] R2.5 Do not persist Stopped until cancellation is acknowledged or a typed timeout is shown.
- [ ] R2.6 Preserve edits/worktrees and report their location after stop.
- [ ] R2.7 On app restart, mark lost processes Interrupted and retain recovery information.
- [ ] R2.8 Test stop during provider silence, tool execution, approval wait, and output streaming.
- [ ] R2.9 Native-test that no child process remains after Stop and after app exit.

**R2 gate:** the visible state and operating-system process state always agree.

## R3 - complete one source-bound solo loop

- [ ] R3.1 Make issue Fix create/focus/select the session and enter Preparing immediately.
- [ ] R3.2 After persistence and worktree provisioning, start execution automatically; no
      second user message is required.
- [ ] R3.3 Send a targeted select-session event after the session exists; do not depend on
      another window's query invalidation.
- [ ] R3.4 Preserve duplicate-source behavior by focusing the matching live session.
- [ ] R3.5 Persist worktree path, branch, base revision, provider, mode, and policy on execution.
- [ ] R3.6 Deliver messages posted during Working through a real steering queue, or label them
      queued for the next turn. Never claim the current run received a message when it did not.
- [ ] R3.7 Convert completion/failure/stop into a durable result automatically.
- [ ] R3.8 Mount result review in the completed conversation and link changed files to Diff.
- [ ] R3.9 Wire Keep, Revise, Undo, Commit, cleanup, and PR draft from that mounted result.
- [ ] R3.10 Run issue Fix from click through reviewed diff and intentional commit in native Tauri.

**R3 gate:** one issue can be fixed from its real repository/host source without opening the
user checkout, and the user reaches a reviewable result without hidden manual steps.

## R4 - app-wide workspace correctness

- [ ] R4.1 List sessions app-wide, grouped by project, with repository filtering as an optional
      filter rather than the identity of the Desk window.
- [ ] R4.2 Do not clear pane selections merely because the current main-window repo changes.
- [ ] R4.3 Allow primary and secondary panes to show sessions from different repositories.
- [ ] R4.4 Resolve repo context per session for source, context, graph, composer, and result.
- [ ] R4.5 Preserve independent drafts and transcript positions across repo changes/restart.
- [ ] R4.6 Test two repos, two running sessions, cross-project Split View, and main-window repo
      switching without selection loss.

**R4 gate:** Agent Desk behaves as one app-wide second window.

## R5 - OpenSpec as execution context

- [ ] R5.1 Call the existing context builder when starting an OpenSpec change/task session.
- [ ] R5.2 Include proposal, design, deltas, exact task, progress, source paths, and honest
      missing-document markers in the provider context.
- [ ] R5.3 Persist a context fingerprint/version on the execution.
- [ ] R5.4 Detect source changes before Plan Start and require refresh or explicit acceptance.
- [ ] R5.5 After review acceptance, invoke the existing task writer for the exact task only.
- [ ] R5.6 Refresh main window, Desk source, progress, transcript, and graph projections.
- [ ] R5.7 Test a non-next task, duplicate task numbers, moved/archived changes, no CLI, and
      no OpenSpec repository.

**R5 gate:** the exact OpenSpec task that started the chat controls context and accepted
completion after restart.

## R6 - real lead and helper execution

- [ ] R6.1 Define a typed provider output/tool for proposing a graph.
- [ ] R6.2 Persist the proposal as AwaitingStart and connect Start/Revise/Use solo.
- [ ] R6.3 Launch each dependency-ready helper through the same execution registry as solo runs.
- [ ] R6.4 Enforce per-helper worktree, allowed paths, model, turn/time budget, and done check.
- [ ] R6.5 Persist each helper event before broadcasting; reject stale/duplicate sequences.
- [ ] R6.6 Key approvals by session, execution, and gate ID.
- [ ] R6.7 Integrate completed helpers in completion order and invoke conflict detection there.
- [ ] R6.8 Preserve all sides of conflicts and resume only the selected integration.
- [ ] R6.9 Run the lead's combined review/check before declaring the graph complete.
- [ ] R6.10 Reconstruct or interrupt graphs honestly after restart.

**R6 gate:** two real helpers execute concurrently, can be stopped independently, survive a
restart safely, and preserve both results through a forced same-line conflict.

## R7 - reachable imports and safe configuration sync

- [ ] R7.1 Mount Import from the session sidebar without blocking native session loading.
- [ ] R7.2 Prove import, incremental refresh, Continue here, Continue externally, and provenance
      in the mounted transcript for each enabled adapter.
- [ ] R7.3 Keep OpenChamber hidden until an independently verified fixture exists.
- [ ] R7.4 Add explicit item and destination selection to Agent Setup.
- [ ] R7.5 Show every per-item plan before Match selected apps can apply anything.
- [ ] R7.6 Apply the confirmed plans through the same hash/backup/receipt/Undo path as one item.
- [ ] R7.7 Show pending, partial success, refusal, failure, receipt, and Undo visibly.
- [ ] R7.8 Test concurrent edits and partial batch failure without touching failed destinations.

**R7 gate:** imports are usable from the shipped shell, and configuration writes never occur
without a visible, destination-specific preview and recoverable receipt.

## R8 - release proof

- [ ] R8.1 Run every item in `acceptance-checklist.md` and link named evidence.
- [ ] R8.2 Verify 100%, 125%, 150%, and 200% Windows scaling.
- [ ] R8.3 Verify keyboard-only and screen-reader names, focus return, and reduced motion.
- [ ] R8.4 Profile 1,000 sessions, two live transcripts, and three helpers.
- [ ] R8.5 Verify offline, reconnect, cancellation, crash, restart, and file-lock scenarios.
- [ ] R8.6 Confirm no flow pushes, posts, commits, removes a worktree, or changes external
      configuration without the required explicit user action.
- [ ] R8.7 Remove compatibility code only after old routes and settings have native proof.

**R8 gate:** all suites and native matrices are green, every user action has visible feedback,
and no unchecked acceptance item is described as shipped.

## Required implementation report for every task cluster

The implementing agent must report:

- OpenSpec task IDs changed;
- production entry point and full call chain;
- files changed and why;
- visible success, pending, refusal, and failure behavior;
- automated tests added and exact commands/results;
- native scenarios run, or an explicit statement that they remain unverified;
- current `git status --short`; and
- any capability that exists only in isolation and remains unwired.

Do not use line count, component existence, command registration, generated bindings, or unit
tests alone as proof that a user-facing task is complete.
