# Agent Desk product brief

## Product sentence

Agent Desk is the place where GitWyrm turns a real repository object into an ongoing AI
conversation: right-click an issue and choose Fix, right-click a pull request and choose
Review, or open an OpenSpec step and let a lead agent organize the work.

## The gap

General agent clients start from a blank composer and make the user rebuild context.
Git clients know the repository, branch, host, pull request, issue, diff, checks, and
OpenSpec state but usually stop at displaying them. GitWyrm can join those halves.

The durable advantage is not chat, diffs, worktrees, receipts, or model pickers by
themselves. Those are expected. The advantage is that a session is born from a live,
linkable source inside the user's actual git workflow and can return to that source.

## Window model

Agent Desk is a redesign and expansion of Spec Desk, preserving the second-OS-window
model. It is not a third window. There is one app-wide Agent Desk, not one per repository,
so chats from different projects can share navigation and Split View.

- The main GitWyrm window remains the source browser and launch surface.
- Agent Desk holds dense session navigation, conversation, and execution context.
- OpenSpec becomes a first-class source and planning layer inside Agent Desk rather than
  the entire identity of the window.
- Existing Spec Desk deep links migrate to Agent Desk deep links. Old links continue to
  open the same window during the migration.

## Core object model

Every session is:

`Source + Intent + Conversation + Execution`

- **Source**: repository plus the issue, pull request, OpenSpec task, commit, diff,
  working changes, check failure, or manual start that created the session.
- **Intent**: Fix, Review, Summarize, Explain, Plan, or Ask.
- **Conversation**: user and agent messages, imported message segments, thoughts that a
  provider is allowed to expose, tool activity, approvals, and user steering.
- **Execution**: provider, mode, team shape, run graph, worktrees, changed files, checks,
  and result state.

Source is immutable as provenance. Its cached launch snapshot is kept even when the
live source changes or disappears. The UI shows both the snapshot and current status.

## Main entry points

### Issue

- Primary context action: **Fix with AI**.
- Immediate response: highlight the issue, open/focus Agent Desk, create the session,
  show the source banner, and enter Preparing.
- Default execution: Auto, Lead + helpers when enabled, isolated worktree, branch derived
  from the issue.
- Secondary actions: Plan, Explain, Fix with… provider/team override.

### Pull request

- Primary context actions: **Review with AI** and **Summarize with AI**.
- Review is read-only by default and never creates a worktree unless the user later asks
  to make changes.
- Summarize reads metadata, commits, files, checks, and existing discussion available
  through the host provider.
- Any requested change creates a new execution branch/worktree linked to the same source.

### OpenSpec

- A change or task opens a source-bound session with proposal, design, deltas, tasks, and
  current progress already attached.
- Plan mode lets the lead draft a graph and waits for Start.
- Auto mode lets the lead create helpers when work can be split safely.
- A completed run updates the existing OpenSpec files through their existing writer;
  Agent Desk never invents a shadow task database.

### Manual chat

- New chat starts with the current repository as source.
- The user can attach an issue, PR, change, task, commit, diff, or check later.
- Attaching adds context but does not rewrite the original Started from provenance.

## Conversation shell

- Left: one-line sessions dense enough for dozens per day, with Recent, Project, and
  Diff grouping. Project headers are compact and collapsible.
- Center: one conversation by default, with source banner, chat transcript,
  tool/activity rows, visible provider output, operating mode, team choice, and composer.
  Choosing a chat replaces the visibly active pane; it never creates a hidden tab.
- Split View: two conversation panes. One pane is always visibly active and sidebar
  choices replace only that pane. Closing Split View keeps the active chat. An unsent
  draft belongs to its session and survives pane replacement.
- Message rail: a thin history map on the transcript edge. Hover or keyboard focus opens
  a panel at least half the conversation width; selecting a message scrolls to it and
  flashes the destination.
- Details: each chat header has Source, Context, and Graph icons. Undocked details open
  in an opaque popover for that chat. A detail can be pinned left, right, or bottom and
  then follows the active chat. A left panel can sit above or below the chat list.
- Source bars and pinned details can be hidden independently. The chat-header icons remain
  available, so hiding chrome never hides the information.
- Context includes current-session usage, overall limits when the provider exposes them,
  project/branch/source, context sources, and skills/connectors. Graph becomes prominent
  while multi-agent work runs without being permanently visible for solo chats.
- Stop all belongs in the Graph header and is written out. It is never an unlabeled icon
  beside Send.

## Operating modes and teams

Modes describe authority:

- **Ask**: answer only; no helpers and no file changes.
- **Plan**: inspect and draft a plan/graph; wait for the user to start execution.
- **Auto**: execute ordinary in-repo work and start helpers when useful; ask before the
  existing gated side effects.

Team choice describes shape:

- **Solo agent**: one agent owns the session; no graph.
- **Lead + helpers**: one lead owns the conversation and source, creates bounded helper
  jobs, integrates outputs, and remains responsible for the final answer.

The mode and team controls are independent. Plan + Lead means review the graph before it
starts. Auto + Lead means the lead may start helpers as work is discovered.

## External clients

GitWyrm reads existing local client configuration and session history only through
versioned, read-only adapters. Importing is best effort. GitWyrm never edits another
client's session database.

- Imported messages retain client, external session ID, timestamp, model when known, and
  original project path.
- Imported content is a segment in a GitWyrm session, not flattened into false native
  history.
- Continue externally opens the originating client when supported.
- Continue here starts a new native segment with a handoff summary and preserved source.
- Failure to parse one client cannot block Agent Desk or other adapters.

## Skills and connector manager

This is configuration reconciliation, not a marketplace clone.

- Discover supported clients and their configured skills/MCP servers.
- Show exact source and destinations for every item.
- One-click copy is per skill or connector and per destination.
- Preview the file change before writing it.
- Back up the destination, write atomically, and offer Undo.
- Never copy secrets into a destination that cannot store them safely.

## Non-goals for the first release

- Hosting models or selling usage.
- A cloud account or cross-device chat sync.
- Replacing provider-native sign-in.
- Editing external-client histories.
- Automatically posting PR reviews or issue comments without a separate explicit action.
- Inferring that two tasks are conflict-free from their titles alone.
- Shipping every client adapter before the native Agent Desk flow works end to end.
