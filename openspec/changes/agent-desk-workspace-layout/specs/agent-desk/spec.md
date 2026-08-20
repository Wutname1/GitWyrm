# agent-desk Spec Delta

## ADDED Requirements

### Requirement: A chat selection replaces the active pane

Agent Desk SHALL show one conversation pane by default. Selecting a session or New chat
SHALL replace the session shown in the active pane, not create a hidden tab or another OS
window. The pane and row SHALL acknowledge the selection immediately.

#### Scenario: Select another chat

- WHEN the user selects a different chat from the sidebar
- THEN that chat replaces the active pane and the selected row and pane are visibly marked

#### Scenario: A slow chat cannot win a race

- WHEN chat A loads slowly and the user selects chat B before A finishes
- THEN B remains selected and A's late response cannot replace it

### Requirement: Split View has two explicit targets

Agent Desk SHALL offer a two-pane Split View. Exactly one pane SHALL be active, sidebar
selections SHALL replace only that pane, and collapsing the split SHALL keep the active
session. Selecting a session already visible SHALL focus its pane instead of duplicating
the conversation.

#### Scenario: Replace the right chat

- WHEN Split View is open, the right pane is active, and the user selects a session
- THEN only the right pane changes and receives an immediate active/loading response

#### Scenario: Collapse with right pane active

- WHEN the right pane is active and the user closes Split View
- THEN the right pane's session becomes the single visible conversation

### Requirement: Unsent drafts belong to sessions

An unsent composer draft SHALL be keyed to its session, preserved when a pane is replaced,
and cleared only after that session's Send is accepted. A failed Send SHALL retain the
draft and expose a retry action.

#### Scenario: Return to a draft

- WHEN the user types in chat A, replaces its pane with chat B, and returns to chat A
- THEN chat A's unsent text is restored and chat B's composer remains independent

### Requirement: Session details open from each pane

Every pane SHALL expose named Source, Context, and Graph controls. When undocked, a detail
SHALL open in an opaque popover anchored to that pane and SHALL read only that pane's
session. Closing it SHALL return keyboard focus to its trigger.

#### Scenario: Two panes have different sources

- WHEN issue chat A and pull-request chat B are visible and the user opens B's Source
- THEN the popover names B's pull request and never shows A's issue

### Requirement: Details can be pinned and moved

The detail panel SHALL be pinnable left, right, or bottom and SHALL follow the active
pane's session. The left placement SHALL support above or below the chat list. Pointer
drag SHALL have equivalent menu and keyboard actions. V1 SHALL show at most one pinned
detail panel and SHALL NOT lock it to an inactive chat.

#### Scenario: Active chat changes under a pinned panel

- WHEN Context is pinned and the user activates the other conversation pane
- THEN the panel visibly refreshes to that pane's session without moving or reopening

#### Scenario: Move without dragging

- WHEN the user opens the panel Move menu and chooses Below chats
- THEN the panel moves there immediately and the destination visibly acknowledges it

### Requirement: Source bars and panels can be hidden safely

The large source bars and a pinned detail panel SHALL be independently hideable. Hiding
them SHALL leave the matching per-pane icon available, and the toolbar SHALL show the
current visibility state.

#### Scenario: Hide source bars

- WHEN the user hides source bars in Split View
- THEN both large bars disappear, each pane keeps its Source icon, and the toolbar reports
  that source bars are hidden

### Requirement: Workspace layout survives restart

Split state, active pane, pane session IDs, source-bar visibility, dock kind, dock edge,
left order, and safe panel size SHALL survive restart as harmless UI preferences. Open
popovers, drag state, and hover state SHALL not be restored.

#### Scenario: Restore a docked split

- WHEN the user quits with Split View open and Context pinned below the chat list
- THEN Agent Desk restores that layout with the same valid sessions on next launch

#### Scenario: A selected session no longer exists

- WHEN a persisted pane session was deleted before restart
- THEN the pane selects a valid recent session and explains the fallback without failing

### Requirement: Split View remains usable when space shrinks

Agent Desk SHALL prevent pane composers and controls from being clipped. Split panes SHALL
stack at compact widths and reduce to an active-pane switcher at narrow widths. A pinned
right panel SHALL return to popover access before it makes a chat unusable.

#### Scenario: Resize a split window narrow

- WHEN the user narrows the window while Split View and a right panel are open
- THEN the active chat stays usable and no composer or detail control is unreachable

