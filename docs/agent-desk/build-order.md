# Agent Desk build order

This is the dependency order, not a menu. Each slice must be releasable behind the current
Spec Desk setting and must leave the existing flow working.

> **Second audit - 2026-08-22:** automated Gate 0 is green. Development must now pass the
> safety-convergence order below before advancing by package number. Existing UI and passing
> unit tests do not make event routing, read-only authority, or graph consolidation safe.

## Status legend

- **Existing**: already represented by an active OpenSpec package.
- **New**: package added for this Agent Desk plan.
- **Gate**: evidence required before the next dependent slice begins.

## 0. Finish the floor already in progress

Do not begin the Agent Desk shell by cloning unfinished systems.

### 0A. Existing AI floor

Packages:

- `add-ai-agent-engine`
- `add-ai-default-provider`
- `add-ai-ask-mode`
- `add-ai-run-completion`

Minimum prerequisite, even if unrelated live-provider verification remains manual:

- cancellation is prompt and leaves no orphan;
- one active execution can be reloaded after a window remount;
- provider reconnect is typed and visible;
- finished/stopped/failed outcomes preserve or undo work honestly; and
- stale events are rejected by session ID.

### 0B. Existing git/OpenSpec floor

Packages:

- `add-worktrees`
- `add-spec-editing`
- `add-specs-sidebar-menu`
- `add-ai-change-drafting`

Minimum prerequisite:

- create/list/remove/recover worktrees is stable on Windows;
- the run marker identifies agent-created worktrees;
- OpenSpec files can be read, edited, and refreshed in both windows; and
- right-click menu composition already has reusable components.

**Gate 0:** existing single-agent OpenSpec run works natively from start through stop or
finish, and an isolated worktree survives app restart without losing the only copy.

Current status: **AUTOMATED BASELINE PASSED; NATIVE FLOOR OPEN.** Cancellation is wired and
the automated suites are green. The native start/stop/restart proof remains open.

### Gate 0A. Safety convergence (new controlling order)

This gate sits before the numbered feature packages, even where partial implementations
already exist:

1. Replace repo-keyed event routing with execution-ID-to-session routing and stress two
   concurrent sessions in one repo.
2. Add provider launch-time write denial for Ask, Review, Summarize, Explain, and Plan before
   Start. Keep the runtime permission handler as defense in depth.
3. Give every graph a dedicated lead integration worktree; never consolidate into the user's
   open checkout.
4. Integrate the real helper worktree delta, including uncommitted edits and full git file
   operation/byte/mode semantics.
5. Persist integration operations/conflicts and run a lead combined review/check that builds
   one combined result before Finished.
6. Make every source action start its selected operation and preserve provider/mode/team in
   the first run.
7. Wire revision, exact OpenSpec task acceptance, and startup result/worktree reconciliation.

**Gate 0A:** adversarial read-only and Plan runs leave the checkout byte-identical; two
same-repo sessions never cross-route; two real uncommitted helper results combine in an
isolated integration worktree; delete/rename/binary tests pass; and the graph exposes one
reviewable combined result without touching the user's checkout.

## 1. Durable session foundation

OpenSpec package: `agent-desk-session-foundation`.

Build backend-first:

1. Versioned Rust domain types and serialization fixtures.
2. App-data layout, atomic session writer, index writer, and index rebuild.
3. Create/list/get/rename/archive/read commands with typed outcomes.
4. Session event envelope with sequence and execution IDs.
5. Adapter from current `RunEventKind` to persisted session messages.
6. Frontend queries/store and restart hydration.
7. Compatibility route so Spec Desk links select an OpenSpec-backed session/filter.

Do not redesign the whole screen in this slice. A developer-only session list is enough
to prove persistence.

**Gate 1:** create 1,000 fixture sessions, restart, list in pages, open one, append an
event, restart again, and recover it byte-for-byte. Corrupt one file and prove the other
999 remain available.

## 2. Agent Desk shell and conversation

OpenSpec package: `agent-desk-conversation-shell`.

Order:

1. Route Agent Desk through the existing second window and legacy Spec Desk URL.
2. Build the three-column shell with exact empty/loading/error states.
3. Add dense virtual session rows and Recent/Project/Diff grouping.
4. Add source banner with cached/live states.
5. Render native messages and existing run events in one transcript.
6. Add message-history rail and large jump popup.
7. Add composer, mode control, and Solo/Lead team control.
8. Add Context panel and honest optional usage rows.
9. Add Graph tab placeholder fed from execution records.
10. Move current OpenSpec tabs into a source/detail surface reachable from the session.

**Gate 2:** the mockup's shell can be used with keyboard only; rows stay one line at 150%
display scaling; a 1,000-session list remains responsive; no action is visually silent.

## 2B. Split conversations and panel workspace

OpenSpec package: `agent-desk-workspace-layout`.

Order:

1. Resolve the window model to one app-wide Agent Desk and migrate legacy routing.
2. Add versioned layout state and session-keyed composer drafts.
3. Make single-view chat selection replace the active conversation.
4. Add exactly two panes, visible active targeting, and duplicate-session focus.
5. Add per-pane Source, Context, and Graph icons with pane-scoped popovers.
6. Extract details into a shared host and dock right, bottom, above chats, or below chats.
7. Add drag placement plus equivalent Move menu and keyboard commands.
8. Add source-bar and panel visibility controls.
9. Add compact stacking, narrow pane switching, safe panel fallback, and layout restore.

Do not build a general IDE layout engine. V1 supports two conversation panes and one
docked detail panel. A pinned panel follows the active chat; locking it to an inactive
chat is out of scope.

**Gate 2B:** open two sessions from different repositories, preserve independent drafts,
replace each active target, move Context through every dock with pointer and keyboard,
restart, and recover the layout. At narrow width no composer or detail action is clipped.

## 3. Source-bound issue and PR actions

OpenSpec package: `agent-desk-source-kickoffs`.

Order:

1. Shared `StartAgentSessionRequest` and intent policy in Rust.
2. Shared frontend `useStartAgentSession` implementing immediate feedback order.
3. Issue detail/context menu: Fix with AI, Plan, Explain, Fix with… .
4. PR detail/context menu: Review with AI, Summarize with AI, Review with… .
5. Cached snapshot creation from already-loaded row/detail data.
6. Background source enrichment and changed-since-launch comparison.
7. Worktree provisioning for Fix only.
8. Host-neutral tests against GitHub, GitLab, Bitbucket, and Azure capabilities.
9. Missing/offline/reconnect/duplicate-session recovery.

Do not wait on chat import or graphs. A source-bound solo run is already valuable.

**Gate 3:** with an intentionally slow host/provider, the source changes to Starting and
Agent Desk shows Preparing immediately. Review/Summarize make zero filesystem changes.
Fix writes only in its isolated worktree.

## 4. OpenSpec-native sessions

OpenSpec package: `agent-desk-openspec-workflows`.

Order:

1. OpenSpec change and task source variants.
2. Compatibility mapping from current selected change/run tab.
3. Source context builder using proposal, design, deltas, task, and progress.
4. Plan-mode graph draft schema tied to requirement/task IDs.
5. Existing writer bridge for checkbox and accepted spec edits.
6. Source refresh after file watcher events.
7. Archive/deleted/moved change states.
8. Keep full non-AI handoff actions available.

**Gate 4:** start from a specific non-next task and prove the execution, transcript,
checkbox write, progress, and source banner all name that exact task after restart.

## 5. Lead and helper graph

OpenSpec package: `agent-desk-agent-graphs`.

Order:

1. Proposed graph types and DAG validation.
2. Plan-mode AwaitingStart state and Start/Revise/Use solo actions.
3. Helper execution IDs, budgets, allowed paths, and completion conditions.
4. Mandatory worktree provisioning per helper.
5. Parallel scheduler limited to three helpers.
6. Per-helper event persistence and graph projection.
7. Per-helper stop and room-level Stop all.
8. Central approval queue routed to the originating helper only.
9. Completion-order integration and typed conflict state.
10. Dedicated lead integration worktree and real staged/unstaged/committed helper delta.
11. Operation fidelity for add/modify/delete/rename/binary/symlink/file modes.
12. Durable conflict resolution applied to the integration worktree.
13. Lead combined review/check, helper result links, and one combined result.
14. Restart/recovery of running, waiting, integrating, and conflicted graphs.

Do not add nested helper-created helpers in this release.

**Gate 5:** run two helpers that edit different files, stop one, answer a gate for the
other, restart the app, and finish. Then force both to edit the same line and prove the
conflict preserves both results without committing either silently.

## 6. External chat continuity

OpenSpec package: `agent-desk-external-chat-import`.

Ship one adapter at a time behind its own capability flag:

1. Adapter trait, common import model, fixture harness, and failure isolation.
2. Codex discovery/import/continue externally.
3. Claude Code discovery/import/continue externally.
4. OpenCode discovery/import/continue externally.
5. VS Code Copilot discovery/import/continue externally.
6. OpenChamber discovery/import/continue externally.
7. Project path reconciliation and unresolved-project UI.
8. Import deduplication and incremental refresh cursors.
9. Continue here with handoff summary and preserved provenance.

**Gate 6 per adapter:** test supported, unsupported-version, corrupt-session, missing-path,
and 1,000-session fixtures. Prove the adapter performs no writes to the external client.
An adapter that fails the gate stays hidden without blocking the rest of Agent Desk.

## 7. Skill and connector sync

OpenSpec package: `agent-desk-configuration-sync`.

Order:

1. Reuse adapter detection to read configuration locations.
2. Normalize skill/MCP metadata without normalizing away client-specific fields.
3. Inventory UI showing source, destinations, difference, and secret warning.
4. Per-item/per-destination preview plan.
5. Merge writers for one client at a time.
6. Hash-based concurrent-change refusal.
7. Backup, atomic write, receipt, and Undo.
8. Batch Match selected apps built only on the same per-item plans.

**Gate 7 per client:** round-trip fixtures with comments/unknown fields preserved, abort on
concurrent edit, undo to byte-identical content, and prove secrets are not exposed.

## 8. Review, landing, and hardening

OpenSpec package: `agent-desk-review-and-landing`.

Order:

1. Unified result state over existing diff/check/commit data.
2. Review one helper or combined result in GitWyrm's diff view.
3. Keep/undo/revise actions tied to existing run completion behavior.
4. Intentional commit flow with source/OpenSpec trailers.
5. PR creation/handoff as a separate explicit action; never push implicitly.
6. Session archive and worktree cleanup/recovery.
7. Accessibility, display scaling, restart, offline, and cancellation matrix.
8. Performance pass over 1,000 sessions and large transcripts.
9. Remove obsolete Spec Desk-only shell code after compatibility telemetry/manual checks.

**Gate 8:** every acceptance item in `acceptance-checklist.md` has automated or named
native proof. No compatibility wrapper is removed until old URLs and settings migrate.

## Recommended release slices

1. **Agent Desk Preview:** phases 0-2B. Native sessions, redesigned app-wide second
   window, Split View, and dockable details, still launched mostly from OpenSpec.
2. **One-click AI:** phases 3-4. Issue/PR/OpenSpec source kickoffs with solo execution.
3. **Agent Teams:** phase 5. Lead and helper graphs.
4. **Bring your history:** phase 6, adapter by adapter.
5. **Keep tools aligned:** phase 7, client by client.
6. **General availability:** phase 8.

## Commit discipline for small agents

- One checkbox cluster that leaves tests green per commit.
- Never combine a mechanical rename with behavior.
- Never edit generated bindings by hand.
- Never start two Cargo test/build processes together on Windows.
- Never push unless the user explicitly asks.
- Before each commit: inspect `git diff`, run the smallest relevant tests, run typecheck for
  frontend changes, and verify only intended paths are staged.
