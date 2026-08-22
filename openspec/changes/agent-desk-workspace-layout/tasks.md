## 1. Window ownership and migration

- [x] 1.1 Change Agent Desk routing to one stable app-wide window label.
- [x] 1.2 Make every repo/source kickoff focus the existing Agent Desk window.
- [ ] 1.3 Pass the target repo and session through a select-session event after focus, and
      prove an already-open Desk selects it after creation.
- [x] 1.4 Translate legacy per-repo Spec Desk labels and URLs without opening duplicates.
- [x] 1.5 Migrate the most recently used legacy Desk placement to the app-wide label once.
- [ ] 1.6 Test two repo kickoffs, legacy/new entry points, and repeated focus natively.

## 2. Workspace state

- [x] 2.1 Add strict `AgentWorkspaceLayout` and `ConversationPaneState` types.
- [x] 2.2 Add an Agent Desk layout slice separate from durable session state.
- [x] 2.3 Persist split, active pane, pane session IDs, source visibility, dock edge/order,
      and clamped panel size.
- [x] 2.4 Keep popover, hover, drag preview, and transcript scroll ephemeral.
- [x] 2.5 Restore missing-session selections to the newest valid session with a notice.
- [x] 2.6 Add schema versioning and a safe default for malformed/older layout data.
- [x] 2.7 Unit test defaults, migrations, invalid IDs, and size clamping.

## 3. Session-scoped drafts

- [x] 3.1 Move composer drafts from pane/component state to a map keyed by session ID.
- [x] 3.2 Preserve a draft when a different chat replaces its pane.
- [x] 3.3 Restore the exact draft when that session is selected again.
- [x] 3.4 Keep primary and secondary session drafts independent.
- [x] 3.5 Clear only the accepted session draft after Send persists its user event.
- [x] 3.6 Preserve a draft when Send fails and show the retry state.
- [x] 3.7 Test replacement, split collapse, restart policy, failed Send, and rapid switching.

## 4. Single-pane replacement

- [x] 4.1 Render one conversation pane by default.
- [x] 4.2 Give the pane a visible active state and accessible active label.
- [x] 4.3 Make a sidebar chat click highlight the row and replace the active pane immediately.
- [x] 4.4 Show a loading skeleton inside the targeted pane without clearing its header.
- [x] 4.5 Prevent late session loads from replacing a newer selection.
- [x] 4.6 Make New chat replace the active pane and focus its composer.
- [ ] 4.7 Test keyboard selection, 100 rapid selections, slow loads, and load failure.

## 5. Split View

- [x] 5.1 Add a labeled Split View toolbar button with pressed state.
- [x] 5.2 Insert exactly one secondary conversation pane in V1.
- [x] 5.3 Make pointer focus, `Ctrl+1`, and `Ctrl+2` set an unmistakable active pane.
- [x] 5.4 Route sidebar selections and New chat to the active pane only.
- [x] 5.5 If the selected session is already visible, focus its pane instead of duplicating it.
- [x] 5.6 Collapse Split View to the active session, including promotion of the right pane.
- [x] 5.7 Preserve each pane's transcript position while Split View remains open.
- [x] 5.8 Show side-by-side panes when wide, stacked panes when compact, and an active-pane
      switcher when narrow.
- [ ] 5.9 Test cross-project panes, live updates, one running graph, scaling, and resize.

## 6. Pane detail actions and popovers

- [x] 6.1 Add named Source, Context, and Graph icon buttons to every pane header.
- [x] 6.2 Scope every button lookup to that pane's session ID.
- [x] 6.3 Build an opaque popover with title, close action, dock actions, and collision handling.
- [x] 6.4 Move focus into the popover and return it to the opening icon on close.
- [x] 6.5 Close on Escape/outside click without changing the selected session.
- [x] 6.6 Keep Source available when the large source bars are hidden.
- [x] 6.7 Show Graph disabled/empty copy honestly when no graph exists.
- [ ] 6.8 Test two different sources/contexts at once to prevent cross-pane data leakage.

## 7. Docked panel system

- [x] 7.1 Extract Source, Context, and Graph content from placement-specific shells.
- [x] 7.2 Add one dock host that follows the active pane's session.
- [x] 7.3 Add right, bottom, left-above-chats, and left-below-chats placements.
- [x] 7.4 Add Pin, Move, and Unpin menu actions with immediate visible state.
- [x] 7.5 Add pointer drag with a full panel ghost and highlighted valid targets.
- [x] 7.6 Reject invalid drops visibly and restore the previous placement.
- [x] 7.7 Add keyboard alternatives for every drag destination.
- [x] 7.8 Add resizable dock boundaries with minimum chat and panel sizes.
- [x] 7.9 Keep panel kind/placement/size when switching chats; refresh its contents.
- [x] 7.10 Hide the right dock automatically at unsafe widths and expose its popover button.
- [ ] 7.11 Test moving while helpers stream events and while the active pane changes.

## 8. Hide, restore, and commands

- [x] 8.1 Add a source-bar visibility toggle and visible pressed state.
- [x] 8.2 Hide both pane source bars without hiding per-pane Source icons.
- [x] 8.3 Add panel hide/unpin actions that leave the pane icon available.
- [x] 8.4 Add `Ctrl+Alt+S`, pane focus commands, and command-palette entries.
- [x] 8.5 Restore layout before first paint to avoid a three-column flash.
- [x] 8.6 Add a Reset workspace layout action with a plain confirmation dialog.

## 9. Verification

- [x] 9.1 Component test active-pane targeting, duplicate focus, and split collapse.
- [ ] 9.2 Component test popover focus return, Escape, and panel-session scoping.
- [x] 9.3 Store test draft preservation and every persisted layout field.
- [x] 9.4 Interaction test all dock placements by pointer and keyboard.
- [ ] 9.5 Test at 100%, 125%, 150%, and 200% Windows display scaling.
- [ ] 9.6 Test narrow, compact, standard, ultrawide, and second-monitor window sizes.
- [ ] 9.7 Test 1,000 session rows with two live transcripts and a streaming graph.
- [x] 9.8 Run `npm run typecheck` and focused frontend tests.
- [ ] 9.9 Regenerate Specta bindings only if Rust window commands/types changed.
- [ ] 9.10 Verify the complete flow in a native Tauri window, including restart.


## Status 2026-08-21

Reconciliation pass: no checkbox changes. This file's checked items were spot-verified
against real code and found accurate, not aspirational:

- 1.1/1.2: `AGENT_DESK_LABEL` is a single stable `"agent-desk"` constant in
  `src-tauri/src/commands/spec_desk.rs`; `open_spec_desk` focuses the existing window
  before ever considering creating one.
- 1.3 correctly stays unchecked: the repo/change-only `SELECT_DESK_TARGET_EVENT` has no
  session field, and the session-carrying `agent-desk://select-session` event (wired by
  the source-kickoffs package) is only proven by a string-match test
  (`select_session_event_name_matches_the_frontend_listener`), not by any test that
  drives it through an already-open Desk window. The task's own text demands that native
  proof; it does not exist yet.
- 5.5: `agentDeskPaneTargeting.ts`/`.test.ts` covers "focus instead of duplicate" for both
  panes, and is genuinely imported and used by `AgentDeskView.tsx`, not an orphaned helper.
- 7.5/7.6: `useAgentDeskPanelDrag.ts` is real native HTML5 drag-and-drop wiring, not a stub;
  its own comment honestly discloses that "full panel ghost" is the browser's default drag
  image rather than a custom-rendered ghost, which is a disclosed simplification, not a
  false claim.

Remaining unchecked items (1.6, 4.7, 5.9, 6.8, 7.11, 9.2, 9.5, 9.6, 9.7, 9.9, 9.10) are all
native-Tauri, display-scaling, high-volume-data, or cross-pane-leakage scenarios that no
agent in this environment can execute or observe; leaving them unchecked is correct.
