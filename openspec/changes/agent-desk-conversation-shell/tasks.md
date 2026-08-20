# Tasks

## 1. Window migration

- [ ] 1.1 Add `agent-desk` window mode and legacy `spec-desk` translation.
- [ ] 1.2 Update backend open/focus logic to find new and legacy labels before creating.
- [ ] 1.3 Rename user-facing title/settings copy to Agent Desk while keeping persisted key
      compatibility.
- [ ] 1.4 Add native test that repeated old/new entry points focus one window.

## 2. Shell

- [ ] 2.1 Add `AgentDeskView`, title bar, and responsive three-column layout.
- [ ] 2.2 Add explicit opening, empty, load-failed, and no-repository states.
- [ ] 2.3 Keep current OpenSpec details/actions functional in the center/source detail.
- [ ] 2.4 Confirm every click has selection, pending state, focus movement, or toast.

## 3. Session sidebar

- [ ] 3.1 Add paged virtual session list with 28-32 px one-line rows.
- [ ] 3.2 Add Recent grouping by day without duplicating project entries in data.
- [ ] 3.3 Add Project grouping by normalized repo path and compact collapsible headers.
- [ ] 3.4 Add Diff grouping based on `changed_file_count > 0` and result state.
- [ ] 3.5 Add New chat, rename, archive, unread/working/needs-you states.
- [ ] 3.6 Preserve selected session across refresh and route changes.
- [ ] 3.7 Test 1,000 rows, duplicate repo names, missing repo paths, and long titles.

## 4. Conversation

- [ ] 4.1 Add source banner for every source variant and live/cached/changed states.
- [ ] 4.2 Render user, assistant, tool, approval, result, and imported message kinds.
- [ ] 4.3 Reuse inert markdown rendering; message HTML/scripts never execute.
- [ ] 4.4 Map existing run event links to current diff/worktree/OpenSpec destinations.
- [ ] 4.5 Add transcript auto-follow only when already near bottom; never steal manual scroll.
- [ ] 4.6 Add visible working state during silent provider time.

## 5. Message rail

- [ ] 5.1 Compute rail ticks from user-message offsets after layout and resize.
- [ ] 5.2 Add hover/focus popup at least 50% of transcript width with collision handling.
- [ ] 5.3 Truncate snippets by lines, not by shrinking type.
- [ ] 5.4 Jump, focus, and animate the destination; honor reduced motion.
- [ ] 5.5 Test keyboard access, 1/50/500 user messages, and resized windows.

## 6. Composer and controls

- [ ] 6.1 Add Ask/Plan/Auto control with plain authority descriptions.
- [ ] 6.2 Add Solo/Lead + helpers control independent of operating mode.
- [ ] 6.3 Append sent user messages visibly before backend execution begins.
- [ ] 6.4 Prevent duplicate sends while accepting the message.
- [ ] 6.5 Put labeled Stop all in Graph header only; no ambiguous square beside Send.

## 7. Right panel

- [ ] 7.1 Add Context and Graph tabs with Context default when no graph exists.
- [ ] 7.2 Show project, branch/worktree, original source, and context-source counts.
- [ ] 7.3 Add collapsible usage from optional normalized provider data.
- [ ] 7.4 Omit unknown values and label estimate/report source in accessible details.
- [ ] 7.5 Add Graph empty state explaining Solo, Plan, and Auto behavior.

## 8. Proof

- [ ] 8.1 Add component/store tests for grouping, selection, rendering, and controls.
- [ ] 8.2 Verify 100%, 125%, and 150% Windows scaling in native Tauri.
- [ ] 8.3 Verify keyboard-only path from session list through history jump and composer.
- [ ] 8.4 Run typecheck and relevant tests; record Gate 2 evidence.
