# Change: The Spec Desk popout window (shell + changes list)

## Why

Deep spec work needs room the main window doesn't have. The Desk is a second OS window
the user parks on another monitor: the main window stays a git client, the Desk holds
everything spec. This change ships the window itself and its left column - the shell
that `add-spec-desk-detail` and `add-spec-desk-handoff-actions` fill in.

## What Changes

- A second Tauri window per repository: titlebar ("Spec Desk · <repo>"), Keep on top
  toggle, Show main window button, remembered size/position
- Changes list column: rows with id, updated time, status pill, progress bar;
  Active / Needs review / Mine filters; New change button (behavior in later changes);
  Archive link with count
- Desk status bar: selected change id, "tasks.md watched · saved instantly"
- Cross-window shared selection and live state (both windows render the same data)

## Impact

- Affected specs: `spec-desk` (new capability)
- Affected code: new window entry in `src-tauri` window config, new `SpecDesk` React
  root, shared selection via events/store sync
- Depends on: `add-openspec-foundation`, `add-specs-sidebar-and-card` (shared selection)
