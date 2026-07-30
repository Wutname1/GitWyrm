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

- [x] 3.1 Right-click an unselected row: selection moves, then every item acts on
      that change
- [ ] 3.2 Archive a complete change: row leaves the section, counts drop
- [ ] 3.3 Validate a change with a known problem: the message names it plainly
- [ ] 3.4 Repo without `openspec/` shows no Specs section and no menu
- [x] 3.5 Manual check in a native window

Verification notes: the folder-name preview was checked for parity against the
Rust `sanitize_change_id` across 12 inputs (identical on all, including
empty-result and non-ASCII cases).

2026-07-30, native dev window: the row menu never opened. `SpecRow` and the
Desk's `ChangeRow` were plain components naming only their own props, so the
`onContextMenu` and ref that `ContextMenuTrigger asChild` merges onto its child
were both dropped. Both now `forwardRef` and spread the rest - the sibling rows
that always worked (`SectionItemRow`) already did. Confirmed 3.1 after the fix:
right-clicking an unselected row selects it, and the menu names that change.
The section-header menu was never broken (it wraps a raw `div`, which needs
neither).

3.2-3.4 still open: archiving and the no-`openspec` case both mutate state the
shared dev app was live in, and no change with a known validation problem was
on hand.
