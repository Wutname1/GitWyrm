# Agent Desk acceptance checklist

This list is additive to each OpenSpec package. A checked task without the matching proof
below is not finished.

Implementation reset status, 2026-08-21: **no section below is released or gate-complete.**
Unit-tested types, helpers, commands, and unmounted components are supporting evidence, not
acceptance. Check an item only after the production path is wired and its named automated or
native evidence is linked from the implementation report.

## Shell and sessions

- [ ] Spec Desk entry points open Agent Desk; no third window appears.
- [ ] Repeated Open/Fix/Review actions focus the existing Agent Desk window.
- [ ] A new session appears in the sidebar before provider or host work completes.
- [ ] Session rows remain one line at 100%, 125%, and 150% Windows display scaling.
- [ ] 1,000 generated session headers scroll without visible frame drops.
- [ ] Restart restores selection, title, source, transcript, and execution result.
- [ ] A damaged session file is isolated and the remaining index rebuilds.
- [ ] Agent Desk is one app-wide second window; two repository kickoffs never create two
      Desk windows.

## Workspace layout

- [ ] Single view shows one chat and a sidebar selection replaces it immediately.
- [ ] Split View shows exactly two chats and the active target is unmistakable.
- [ ] A sidebar selection replaces only the active pane; selecting an already-visible
      session focuses its pane instead of duplicating it.
- [ ] Closing Split View keeps the active chat, including when the right pane was active.
- [ ] Unsent drafts survive pane replacement, Split View collapse, and failed Send.
- [ ] Each pane's Source, Context, and Graph buttons show only that pane's session data.
- [ ] A detail can be pinned right, bottom, above chats, or below chats and follows the
      active pane.
- [ ] Every drag placement has a menu and keyboard alternative with visible feedback.
- [ ] Hiding source bars or a pinned panel leaves the matching pane icon available.
- [ ] Split, sessions, active pane, source visibility, dock location/order, and safe size
      survive restart; open popovers and drag previews do not.
- [ ] Compact and narrow layouts keep both composers and all panel actions reachable.

## Source and kickoff

- [ ] Issue Fix visibly enters Preparing within one animation frame.
- [ ] Pull request Review and Summarize do not create or modify a worktree.
- [ ] Fix never edits the checkout the user has open.
- [ ] The source banner survives source deletion and labels the cached snapshot honestly.
- [ ] Refresh shows when the live source changed since launch.
- [ ] Duplicate kickoff offers/focuses the existing matching session instead of silently
      creating a second run.
- [ ] Missing provider, expired sign-in, offline host, and worktree failure each leave a
      usable session with a clear next action.

## Conversation

- [ ] User Send appends immediately and disables duplicate submission until accepted.
- [ ] Ask cannot edit; Plan cannot execute before Start; Auto still obeys approval gates.
- [ ] Solo produces no graph; Lead + helpers makes the graph entry available.
- [ ] Message rail includes user messages only, opens by hover and keyboard focus, is at
      least half the transcript width, and jumps/flashes the target.
- [ ] Imported messages show their source client and are never presented as native output.
- [ ] Provider output is labeled accurately; no invented hidden reasoning is shown.

## Context and usage

- [ ] Context is the default right panel before a graph exists.
- [ ] Graph becomes prominent while helpers run without destroying Context state.
- [ ] Missing usage values are omitted, not displayed as zero.
- [ ] Measured, provider-reported, and estimated values are distinguishable in details.
- [ ] Collapsing Usage is remembered per user.

## Graph execution

- [ ] Each helper has a unique worktree, branch, execution ID, and event sequence.
- [ ] Stopping one helper does not stop peers.
- [ ] Stop all is labeled in the Graph header and responds immediately.
- [ ] A stale helper event cannot land in a newer session or execution.
- [ ] A helper conflict pauses only that integration and preserves both sides.
- [ ] Closing and reopening Agent Desk reconstructs the live graph from backend state.
- [ ] A crashed app can recover every worktree that contains the only copy of work.

## OpenSpec

- [ ] Proposal, design, deltas, tasks, and progress come from repository files.
- [ ] A task completion uses the existing OpenSpec writer and refreshes every surface.
- [ ] Plan graph nodes link back to the requirement/task that caused them.
- [ ] A repo without OpenSpec still has a fully usable Agent Desk.
- [ ] OpenSpec CLI absence never blocks reading or source-bound chat.

## External clients

- [ ] Each adapter is read-only and version-gated.
- [ ] One corrupt client session does not break that adapter's remaining sessions.
- [ ] One broken adapter does not block native sessions or other adapters.
- [ ] Imported project paths resolve to known repos when possible and remain visible when
      unresolved.
- [ ] Continue externally never pretends success when the client cannot accept a session.

## Configuration sync

- [ ] Scan shows exact source and destination applications.
- [ ] Copy is per item and destination, with a preview.
- [ ] Destination changes after preview cause a safe refusal.
- [ ] Every write has a backup and working Undo.
- [ ] Secret-bearing configuration is redacted and never copied into plain text silently.

## Safety and release

- [ ] No action pushes.
- [ ] No typed confirmation exists.
- [ ] No user-facing competitor/client names appear outside import/setup surfaces where
      naming the detected client is required.
- [ ] `npm run typecheck` passes.
- [ ] Relevant Rust tests pass from `src-tauri` without a second concurrent Cargo build.
- [ ] Specta bindings are regenerated after Rust command/type registration.
- [ ] Native Tauri verification covers window focus, restart, and provider cancellation.
