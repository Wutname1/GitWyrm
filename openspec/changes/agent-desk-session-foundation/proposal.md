# Change: Add durable Agent Desk sessions

## Why

The current AI run is repository-scoped transient state. Agent Desk needs durable chats
that survive restart, can be listed across projects, and can attach more than one
execution without losing the source that started them.

## What Changes

- Add a versioned session/source/message/execution model in the Rust backend.
- Store one atomic JSON file per session plus a rebuildable compact index in app data.
- Persist current AI run events into sessions with sequence and execution IDs.
- Add typed commands for session create/list/read/update/archive.
- Add frontend queries and live-event overlay without making Zustand the database.
- Accept legacy Spec Desk routes while the UI migration is incomplete.

## Impact

- New backend module: `src-tauri/src/agentdesk/`.
- New commands: `src-tauri/src/commands/agent_desk.rs`.
- New stores/hooks under `src/stores/` and `src/hooks/`.
- Existing `airun` behavior remains the execution engine.
