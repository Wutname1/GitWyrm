# Design

## Decisions

- Use app-data JSON files, not `settings.json` and not a new SQLite dependency.
- One file per session limits corruption and makes migrations incremental.
- The index contains headers only and can be rebuilt from session files.
- Backend commands own canonical content. Frontend state overlays live events only.
- Persist an event before emitting it to the UI.
- Source provenance is a tagged enum with a cached launch snapshot.
- Existing `RunEventKind` is adapted, not replaced, until current run surfaces migrate.
- IDs are generated in Rust. Timestamps are UTC RFC 3339. Event sequence is monotonic per
  execution.

Full types and command list are in `docs/agent-desk/architecture.md` sections 2-4.

## Failure behavior

- Invalid index: rebuild and report a recoverable diagnostic.
- Invalid session file: omit that session, retain the path/reason in diagnostics, keep all
  other sessions usable.
- Atomic rename failure: leave the previous file untouched and return a typed write result.
- Duplicate event sequence: ignore it. Sequence gap: persist and expose a diagnostic flag.
- Event for replaced execution: ignore it in both backend and frontend.
