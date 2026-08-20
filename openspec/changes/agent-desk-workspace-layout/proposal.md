# Change: Add split conversations and dockable session panels

## Why

Agent Desk is chat-first, but a single fixed conversation plus a permanent right column
does not fit real work. People need to compare two conversations, replace the chat they
are looking at without opening hidden tabs, and decide whether Source, Context, or Graph
deserves permanent space. The workspace must stay dense when dozens of sessions cross
several repositories.

## What changes

- Make Agent Desk one app-wide second window instead of one window per repository.
- Keep one conversation pane by default; choosing a chat replaces the active pane.
- Add an explicit two-pane Split View with an unmistakable active target.
- Put Source, Context, and Graph actions in each conversation header.
- Open undocked details as a pane-scoped popover and allow them to be pinned left,
  right, or bottom.
- Let the left dock sit above or below the chat list.
- Allow the source bars and docked detail panel to be hidden independently.
- Preserve session-scoped drafts when a pane is replaced.
- Persist workspace layout without mixing it into durable conversation content.

## Impact

- Affected specs: `agent-desk`, `spec-desk`, `app-windows`.
- Affected frontend: Agent Desk shell, session sidebar, pane headers, composer drafts,
  workspace layout store, keyboard commands, and drag/drop panel chrome.
- Affected backend: app-wide Agent Desk window routing and harmless layout preference
  persistence only. Session content remains owned by the session foundation.
- Depends on: `agent-desk-session-foundation`, `agent-desk-conversation-shell`.

