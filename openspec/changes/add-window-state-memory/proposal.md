# Change: Windows reopen where you left them

## Why

The Spec Desk is meant to live on a second monitor. Today it opens at its default
size in the middle of the primary display every single time, so anyone using it
the intended way has to drag and resize it on every launch. The main window has
the same gap: it is centered at 1400x900 on each start regardless of how the user
had it arranged.

`add-spec-desk-window` claimed Tauri remembered this per window label. It does
not - that behavior needs the window-state plugin, which is not installed. This
change makes the claim true.

## What Changes

- Add `tauri-plugin-window-state` and enable it for the main window and every
  `spec-desk-*` window
- Persist size, position, and maximized state per window label, so each
  repository's Desk remembers its own placement
- Guard against restoring a window onto a monitor that is no longer attached: a
  Desk saved on an unplugged second screen has to come back somewhere visible
- Fix the false claim in the `spec-desk` spec (the requirement currently says
  size and position are remembered)

## Impact

- Affected specs: `spec-desk` (correct an existing requirement), `app-windows` (new)
- Affected code: `src-tauri/Cargo.toml`, `src-tauri/src/lib.rs` (plugin
  registration), `src-tauri/src/commands/spec_desk.rs` (stop setting a fixed size
  when a saved one exists), `src-tauri/capabilities/default.json`
- Depends on: `add-spec-desk-window`
