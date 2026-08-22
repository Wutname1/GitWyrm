# Tasks

## 1. Window migration

- [x] 1.1 Add `agent-desk` window mode and legacy `spec-desk` translation.
- [x] 1.2 Update backend open/focus logic to find new and legacy labels before creating.
- [x] 1.3 Rename user-facing title/settings copy to Agent Desk while keeping persisted key
      compatibility.
- [ ] 1.4 Add native test that repeated old/new entry points focus one window.

## 2. Shell

- [x] 2.1 Add `AgentDeskView`, title bar, and responsive three-column layout.
- [x] 2.2 Add explicit opening, empty, load-failed, and no-repository states.
- [x] 2.3 Keep current OpenSpec details/actions functional in the center/source detail.
- [x] 2.4 Confirm every click has selection, pending state, focus movement, or toast.

## 3. Session sidebar

- [x] 3.1 Add paged virtual session list with 28-32 px one-line rows.
- [x] 3.2 Add Recent grouping by day without duplicating project entries in data.
- [x] 3.3 Add Project grouping by normalized repo path and compact collapsible headers.
- [x] 3.4 Add Diff grouping based on `changed_file_count > 0` and result state.
- [x] 3.5 Add New chat, rename, archive, unread/working/needs-you states.
- [x] 3.6 Preserve selected session across refresh and route changes.
- [x] 3.7 Test 1,000 rows, duplicate repo names, missing repo paths, and long titles.

## 4. Conversation

- [x] 4.1 Add source banner for every source variant and live/cached/changed states.
- [x] 4.2 Render user, assistant, tool, approval, result, and imported message kinds.
- [x] 4.3 Reuse inert markdown rendering; message HTML/scripts never execute.
- [ ] 4.4 Map existing run event links to current diff/worktree/OpenSpec destinations.
- [x] 4.5 Add transcript auto-follow only when already near bottom; never steal manual scroll.
- [x] 4.6 Add visible working state during silent provider time.

## 5. Message rail

- [x] 5.1 Compute rail ticks from user-message offsets after layout and resize.
- [x] 5.2 Add hover/focus popup at least 50% of transcript width with collision handling.
- [x] 5.3 Truncate snippets by lines, not by shrinking type.
- [x] 5.4 Jump, focus, and animate the destination; honor reduced motion.
- [x] 5.5 Test keyboard access, 1/50/500 user messages, and resized windows.

## 6. Composer and controls

- [x] 6.1 Wire Ask/Plan/Auto control to live authority and execution behavior, with plain descriptions.
- [x] 6.2 Wire Solo/Lead + helpers control to live execution behavior independent of operating mode.
- [x] 6.3 Append sent user messages visibly before backend execution begins.
- [x] 6.4 Prevent duplicate sends while accepting the message.
- [x] 6.5 Put labeled Stop all in Graph header only; no ambiguous square beside Send.

## 7. Right panel

- [x] 7.1 Add Context and Graph tabs with Context default when no graph exists.
- [x] 7.2 Show project, branch/worktree, original source, and context-source counts.
- [x] 7.3 Add collapsible usage from optional normalized provider data.
- [x] 7.4 Omit unknown values and label estimate/report source in accessible details.
- [x] 7.5 Add Graph empty state explaining Solo, Plan, and Auto behavior.

## 8. Proof

- [x] 8.1 Add component/store tests for grouping, selection, rendering, and controls.
- [ ] 8.2 Verify 100%, 125%, and 150% Windows scaling in native Tauri.
- [ ] 8.3 Verify keyboard-only path from session list through history jump and composer.
- [ ] 8.4 Run typecheck and relevant tests; record current Gate 2 evidence after all suites pass.

## Gate 2 evidence (automated portion)

Recorded 2026-08-20. Automated proof only - the native items below are still open.

- `npm run typecheck` (tsc -b --force): passes clean.
- `npm run test:unit`: 464 tests across 40 files, 0 failures.
- `cargo test --lib`: 749 tests, 0 failures.
- 1,000-session list behaviour is covered by `src/lib/agentSessionGrouping.test.ts`
  (1,000 rows, duplicate repo names, missing repo paths, long titles).
- Grouping, selection, shell states, rail maths, scroll follow, target resolution,
  composer guards and usage honesty each have pure-logic tests.

Still requires a human at the keyboard, and NOT claimed here:
- 1.4 repeated old/new entry points focus one window (native).
- 8.2 100/125/150% Windows display scaling (native).
- 8.3 keyboard-only path from session list through history jump to composer (native).
- 4.4 stays open: message targets resolve honestly but no diff/worktree/graph
  destination is reachable yet, so nothing can be proven to navigate.

## Status 2026-08-21

Ticked 6.1 and 6.2 during this reconciliation pass. Evidence: `SessionComposer.tsx`
maps the Ask/Plan/Auto pill through `modeToExecutionMode` and the Solo/Lead+helpers
pill through `teamToExecutionTeam` (`src/lib/agentDeskComposer.ts`), both passed as
real arguments to the `agentSessionStartExecution` command. On the Rust side,
`start_execution_at` (`src-tauri/src/commands/agent_desk.rs`) reads those values and
calls `ExecutionPolicy::resolve(intent, mode, team, provider_override)`
(`src-tauri/src/agentdesk/policy.rs`) *before* any worktree/CLI/write side effect,
and an unsupported provider override returns `StartExecutionOutcome::UnsupportedProvider`
rather than silently falling back. This is the same policy plumbing R1 landed and
`cli_run.rs` enforces at the ACP boundary. Plain-language mode notes
(`MODE_NOTES` in `agentDeskComposer.ts`) are shown next to the pills per 6.1.

Everything else in this file was already accurately checked/unchecked before this
pass; no other false positives found. 4.4 remains correctly unchecked -- verified
`agentDeskRail.ts` only computes intra-transcript jump targets, nothing resolves a
diff/worktree/OpenSpec destination from a rail tick.
