# Change: Add the OpenSpec foundation (parse, watch, write back)

## Why

Every Specs surface in the mockup - sidebar rows, progress rings, the Desk, AI runs -
renders the same thing: the `openspec/` folder in the user's repository. Before any UI
ships, GitWyrm needs one Rust module that detects that folder, parses it into typed
data, watches it for edits from any tool, and writes back the few things a human would
write. Without this, each UI change would grow its own ad-hoc parsing and the "files
are the truth" promise breaks.

## What Changes

- New `src-tauri/src/commands/openspec.rs` module: detection, change/spec parsing,
  progress computation, file watching, task checkbox write-back
- `openspec` CLI detection plus a bundled read-only fallback parser
- Typed commands exposed through specta bindings (`list_changes`, `get_change`,
  `toggle_task`, `validate_change`, `archive_change`, `openspec_status`)
- No UI in this change - it is the base every later change builds on

## Impact

- Affected specs: `openspec-core` (new capability)
- Affected code: `src-tauri/src/commands/` (new module), bindings regeneration,
  file-watcher plumbing shared with the existing repo watcher
- Depends on: nothing. Everything else in this plan depends on it.
