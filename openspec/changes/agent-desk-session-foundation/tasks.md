# Tasks

## 1. Domain model

- [ ] 1.1 Add `src-tauri/src/agentdesk/mod.rs`, `model.rs`, `store.rs`, and `events.rs`.
- [ ] 1.2 Define Specta/serde types for header, full session, source variants, intent,
      state, segment, message, attachment, execution, and import provenance.
- [ ] 1.3 Put `schema_version` on the file and add a migration function even though v1 has
      no previous data.
- [ ] 1.4 Add round-trip tests for every source variant and message kind.
- [ ] 1.5 Add fixtures proving unknown enum variants/fields fail or default intentionally,
      never accidentally.

## 2. Persistence

- [ ] 2.1 Resolve `<app-data>/agent-desk/v1` through `settings::app_data_dir`.
- [ ] 2.2 Write session files through temp + flush + atomic rename.
- [ ] 2.3 Write `index.json` from headers using the same atomic path.
- [ ] 2.4 Rebuild the index by scanning session files when missing or invalid.
- [ ] 2.5 Quarantine only the bad file logically; do not move/delete user data automatically.
- [ ] 2.6 Sort headers by `updated_at` descending with stable session-ID tie break.
- [ ] 2.7 Add paged list filters for repo, project path, state, source kind, changed files,
      archived, and text title match.
- [ ] 2.8 Test 1,000 sessions, one corrupt file, interrupted temp file, and duplicate ID.

## 3. Commands and bindings

- [ ] 3.1 Add create/list/get/rename/archive/mark-read commands with typed outcomes.
- [ ] 3.2 Add append-user-message and attach-context commands.
- [ ] 3.3 Register commands/types in `src-tauri/src/lib.rs`.
- [ ] 3.4 Regenerate bindings with the export command; do not hand-edit them.
- [ ] 3.5 Add command integration tests against a temporary app-data root.

## 4. Run bridge

- [ ] 4.1 Add execution ID and sequence to the durable event envelope.
- [ ] 4.2 Map each existing `RunStep` variant to a session message without losing typed data.
- [ ] 4.3 Save the mapped event before emitting `agent-session-event`.
- [ ] 4.4 Reject stale execution events and duplicate sequences in Rust tests.
- [ ] 4.5 Keep `ai-run-event` unchanged until all current consumers migrate.

## 5. Frontend data layer

- [ ] 5.1 Add `agentSessionStore.ts` keyed by session and sequence.
- [ ] 5.2 Add paged `useAgentSessions` and detail `useAgentSession` queries.
- [ ] 5.3 Add one listener at app root and make remount/unlisten idempotent.
- [ ] 5.4 Merge live events without duplicating persisted events after query refresh.
- [ ] 5.5 Add tests for stale event, duplicate event, gap, restart hydration, and two windows.

## 6. Compatibility and proof

- [ ] 6.1 Extend window routing to accept `agent-desk` while keeping `spec-desk` valid.
- [ ] 6.2 Add a temporary developer session list/read surface; no final styling in this change.
- [ ] 6.3 Run Rust unit/integration tests and `npm run typecheck`.
- [ ] 6.4 Native test: create, append, close both windows, relaunch, and recover.
- [ ] 6.5 Record Gate 1 evidence from `docs/agent-desk/build-order.md` in this file.
