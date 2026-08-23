# Tasks

## 1. Domain model

- [x] 1.1 Add `src-tauri/src/agentdesk/mod.rs`, `model.rs`, `store.rs`, and `events.rs`.
- [x] 1.2 Define Specta/serde types for header, full session, source variants, intent,
      state, segment, message, attachment, execution, and import provenance.
- [x] 1.3 Put `schema_version` on the file and add a migration function even though v1 has
      no previous data.
- [x] 1.4 Add round-trip tests for every source variant and message kind.
- [x] 1.5 Add fixtures proving unknown enum variants/fields fail or default intentionally,
      never accidentally.

## 2. Persistence

- [x] 2.1 Resolve `<app-data>/agent-desk/v1` through `settings::app_data_dir`.
- [x] 2.2 Write session files through temp + flush + atomic rename.
- [x] 2.3 Write `index.json` from headers using the same atomic path.
- [x] 2.4 Rebuild the index by scanning session files when missing or invalid.
- [x] 2.5 Quarantine only the bad file logically; do not move/delete user data automatically.
- [x] 2.6 Sort headers by `updated_at` descending with stable session-ID tie break.
- [x] 2.7 Add paged list filters for repo, project path, state, source kind, changed files,
      archived, and text title match.
- [x] 2.8 Test 1,000 sessions, one corrupt file, interrupted temp file, and duplicate ID.

## 3. Commands and bindings

- [x] 3.1 Add create/list/get/rename/archive/mark-read commands with typed outcomes.
- [x] 3.2 Add append-user-message and attach-context commands.
- [x] 3.3 Register commands/types in `src-tauri/src/lib.rs`.
- [x] 3.4 Regenerate bindings with the export command; do not hand-edit them.
- [x] 3.5 Add command integration tests against a temporary app-data root.

## 4. Run bridge

- [x] 4.1 Add execution ID and sequence to the durable event envelope.
- [x] 4.2 Map each existing `RunStep` variant to a session message without losing typed data.
- [x] 4.3 Save the mapped event before emitting `agent-session-event`.
- [x] 4.4 Reject stale execution events and duplicate sequences in Rust tests.
- [x] 4.5 Keep `ai-run-event` unchanged until all current consumers migrate.
- [x] 4.6 Replace repository-keyed run links with execution-ID-to-session links. Repository
      identity may resolve context but must never choose an event's destination session.
      `RunSessionLinks` (`agentdesk/bridge.rs:65`) is keyed `execution_id -> session_id`, not
      by repository -- its own doc comment names this exact fix ("the P0 fix for 'an event
      can reach the wrong chat'... the previous `HashMap<repo_id, SessionId>` shape meant
      linking session B's execution silently stole routing for every future event that
      happened to name the same `repo_id`"). `route_run_event` consults only this map.
      Proven by `linking_a_second_execution_does_not_overwrite_the_first` and the
      cross-contamination tests cited under 4.7.
- [ ] 4.7 Test two concurrent lead/helper executions in separate sessions for the same repo,
      including duplicate, late, terminal, unlink, and unknown execution events. Prove no
      transcript, result, gate, or graph state crosses sessions.
      PARTIAL: the core cross-contamination claim IS proven by two real tests --
      `an_event_for_execution_a_lands_in_session_a_only_when_two_sessions_share_a_repository`
      (also proves unlink isolation: unlinking A's execution leaves B's mapping intact) and
      `a_helper_and_its_lead_in_different_sessions_of_the_same_repo_do_not_cross_contaminate`
      (interleaved lead+helper events, same repo, different sessions, each lands only in its
      own session). Duplicate/late/unknown-execution handling is separately tested elsewhere
      in the same file (`a_duplicate_sequence_is_ignored`,
      `an_earlier_sequence_arriving_late_is_also_a_duplicate`,
      `an_unlinked_repository_routes_to_nothing`) but as single-session cases, not combined
      into the two-concurrent-session scenario this task asks for. No test exercises
      duplicate/late/terminal/unknown events specifically WITHIN the concurrent lead+helper
      cross-session setup. Left unchecked for that missing combination.

## 5. Frontend data layer

- [x] 5.1 Add `agentSessionStore.ts` keyed by session and sequence.
- [x] 5.2 Add paged `useAgentSessions` and detail `useAgentSession` queries.
- [x] 5.3 Add one listener at app root and make remount/unlisten idempotent.
- [x] 5.4 Merge live events without duplicating persisted events after query refresh.
- [x] 5.5 Add tests for stale event, duplicate event, gap, restart hydration, and two windows.

## 6. Compatibility and proof

- [x] 6.1 Extend window routing to accept `agent-desk` while keeping `spec-desk` valid.
- [x] 6.2 Add a temporary developer session list/read surface; no final styling in this change.
- [x] 6.3 Run Rust unit/integration tests and `npm run typecheck`.
- [ ] 6.4 Native test: create, append, close both windows, relaunch, and recover.
      NOT DONE: requires a human to launch the packaged app and drive both windows by
      hand. Automated tests cannot substitute for this.
- [ ] 6.5 Record Gate 1 evidence from `docs/agent-desk/build-order.md` in this file.
      NOT DONE: Gate 1 (`docs/agent-desk/build-order.md`) explicitly requires a real
      restart/recover cycle across app launches, which is the same manual step as 6.4.
      No automated run can honestly stand in for it.
