# Tasks

## 1. Backend

- [x] 1.1 Detect `openspec/` at the repo root and expose an `openspec_status` command
      (present/absent, CLI installed + version)
- [x] 1.2 Parse `changes/<id>/`: proposal.md (Why / What Changes / Impact), tasks.md
      (groups + checkboxes), design.md presence, spec deltas (ADDED/MODIFIED/REMOVED
      requirements with scenarios)
- [x] 1.3 Compute progress (done/total, percent, draft when no tasks) and a derived
      status (Draft / In build / Needs review / Ready to archive)
- [x] 1.4 Watch `openspec/` and re-emit parsed state within 1s of any file change
- [x] 1.5 Task write-back: toggle one checkbox, leave every other byte of tasks.md alone
- [x] 1.6 Shell out to the `openspec` CLI for validate/archive when installed; return a
      typed "CLI missing" outcome otherwise

## 2. Bindings and types

- [x] 2.1 Specta types for Change, Task, Delta, Status; regenerate bindings.ts
- [x] 2.2 TanStack Query hooks (`useOpenspecChanges`, `useOpenspecStatus`) with
      watcher-driven invalidation

## 3. Verify

- [x] 3.1 Unit tests: parser fixtures (well-formed, missing sections, malformed markdown)
- [x] 3.2 Unit test: write-back preserves file content byte-for-byte outside the toggled box
- [ ] 3.3 Manual: edit tasks.md in VS Code while GitWyrm is open; state updates within 1s
