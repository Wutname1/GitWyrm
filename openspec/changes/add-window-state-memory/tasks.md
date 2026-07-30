# Tasks

## 1. Plugin

- [x] 1.1 Add `tauri-plugin-window-state` and register it, scoped to the main
      window and `spec-desk-*` labels
- [x] 1.2 Grant the plugin's permissions in `capabilities/default.json`
- [x] 1.3 Stop `open_spec_desk` forcing 940x760 when a saved size exists; keep it
      as the first-open default

## 2. Safety

- [x] 2.1 Clamp a restored position onto a currently-attached monitor, so a Desk
      saved on an unplugged screen still comes back visible
- [x] 2.2 Restore maximized state without restoring a zero or off-screen size

## 3. Spec correction

- [x] 3.1 Update the `spec-desk` requirement that claims size and position are
      already remembered

## 4. Verify

- [x] 4.1 Move and resize the Desk, close it, reopen: it lands where it was left
- [x] 4.2 Each repository's Desk remembers its own placement independently
- [x] 4.3 Main window position and maximized state survive a restart
- [x] 4.4 Unplug/disable the second monitor with a Desk saved there; it reopens
      on an attached display. Needs a human with a second monitor to unplug.
      The mechanism is in place: the plugin only restores a position that a
      currently-attached monitor intersects, and otherwise lets the OS place the
      window (see `restore_state` in tauri-plugin-window-state 2.4.1).
