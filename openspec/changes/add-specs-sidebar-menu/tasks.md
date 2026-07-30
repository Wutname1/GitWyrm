# Tasks

## 1. Shared menu

- [x] 1.1 `SpecContextMenu` component wrapping a row: open in Desk, copy
      next-task handoff, copy change id, validate
- [x] 1.2 Right-click selects the row's change before the menu opens. Uses
      `ContextMenu onOpenChange`, not an `onContextMenu` prop on the trigger:
      `asChild` merges props onto the child and Radix owns that handler there.
- [x] 1.3 Handoff item absent when no task is open; archive item absent unless
      every task is done
- [x] 1.4 Archive confirms with a single click (no type-to-confirm) and names
      the change

## 2. Wiring

- [x] 2.1 Wrap Specs sidebar rows in the menu
- [x] 2.2 Reuse the same menu on the Desk's changes list, so both lists answer a
      right-click the same way
- [x] 2.3 Section-header menu: new change, refresh. "New change" needed a real
      modal built over the existing (previously unused) scaffold command; the
      Desk's "+ New" button now opens it too instead of a placeholder toast.

## 3. Verify

- [ ] 3.1 Right-click an unselected row: selection moves, then every item acts on
      that change
- [ ] 3.2 Archive a complete change: row leaves the section, counts drop
- [ ] 3.3 Validate a change with a known problem: the message names it plainly
- [ ] 3.4 Repo without `openspec/` shows no Specs section and no menu
- [ ] 3.5 Manual check in a native window

Verification notes: the folder-name preview was checked for parity against the
Rust `sanitize_change_id` across 12 inputs (identical on all, including
empty-result and non-ASCII cases). The rest of section 3 needs a native window -
the app cannot run in a browser preview at all (a Tauri-only call in `AppInner`
throws before any of this renders), and the shared dev app was in use.
