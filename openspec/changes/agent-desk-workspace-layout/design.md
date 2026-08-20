# Design: Agent Desk workspace layout

## Decisions

### One app-wide Desk

There is one Agent Desk window for the GitWyrm application. Repository, issue, PR, and
OpenSpec actions focus that window and select or create the relevant session. This makes
cross-project chat navigation and a two-project split possible without multiplying OS
windows. The main GitWyrm window remains the repository browser and kickoff surface.

### Panes select views of sessions

A pane does not own a conversation. It stores only a selected session ID and transient
viewport state. The durable session is loaded through the session query/store. In single
view, a sidebar selection replaces the sole pane. In Split View, it replaces the active
pane. If that session is already visible in the other pane, focus that pane instead of
showing a misleading duplicate.

Closing Split View keeps the active session. If the right pane was active, it becomes the
single pane. An unsent composer draft is keyed by session ID, not pane ID, so replacing a
pane never discards it. Drafts may be stored locally but never become transcript events
until Send is accepted.

### Details belong to the pane that opened them

Each pane header exposes Source, Context, and Graph icon buttons. An undocked panel is an
opaque, keyboard reachable popover anchored to that pane. It reads the pane's session ID;
it must not fall back to a global selected source.

A pinned panel follows the active pane by default. V1 does not include locking a panel to
an inactive session. Only one details dock is visible at a time; switching detail kind
reuses it. The user may dock it left, right, or bottom. A left dock may be placed above or
below the session list. Dragging is a convenience: every drag result is also available
from a plain menu and keyboard.

### Layout state is harmless UI state

Persist one `AgentWorkspaceLayout` per user installation:

```ts
interface AgentWorkspaceLayout {
  split: boolean
  activePane: 'primary' | 'secondary'
  primarySessionId: string | null
  secondarySessionId: string | null
  sourceBarsVisible: boolean
  dock: null | {
    kind: 'source' | 'context' | 'graph'
    edge: 'left' | 'right' | 'bottom'
    leftOrder?: 'above-chats' | 'below-chats'
    sizePx: number
  }
}
```

Open popovers, focus rings, drag previews, transcript scroll, and hover state are not
persisted. Clamp restored sizes to the current window and attached monitor. Missing or
deleted sessions fall back to the newest available session with a visible notice.

### Responsive behavior

Wide layouts show two side-by-side panes. At the compact breakpoint, Split View stacks
the panes vertically rather than clipping either composer. At the narrow breakpoint,
the active pane is shown and a pane switcher remains available. A right dock becomes a
popover before it can make a conversation unusable.

## Visible response rules

- Clicking a chat highlights the row and active pane immediately, then loads content.
- Toggling Split View inserts/removes the second pane in the same frame.
- Starting a drag shows the full panel ghost and valid targets; dropping flashes the new
  dock. Invalid drops return the panel to its origin and explain why.
- Hiding source bars changes the toolbar state and leaves the per-pane Source icon.
- Pinning, moving, resizing, and unpinning panels always update visible chrome first.

## Accessibility

Pane and panel controls have names and pressed/selected state. `Ctrl+Alt+S` toggles Split
View, `Ctrl+1` and `Ctrl+2` focus panes, and panel commands are available from the pane
header menu. Drag handles are not the only way to move a panel. Focus moves into an opened
popover and returns to its trigger when closed.

