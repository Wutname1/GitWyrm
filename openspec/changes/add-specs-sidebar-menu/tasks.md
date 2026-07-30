# Tasks

## 1. Shared menu

- [ ] 1.1 `SpecContextMenu` component wrapping a row: open in Desk, copy
      next-task handoff, copy change id, validate
- [ ] 1.2 Right-click selects the row's change before the menu opens
- [ ] 1.3 Handoff item absent when no task is open; archive item absent unless
      every task is done
- [ ] 1.4 Archive confirms with a single click (no type-to-confirm) and names
      the change

## 2. Wiring

- [ ] 2.1 Wrap Specs sidebar rows in the menu
- [ ] 2.2 Reuse the same menu on the Desk's changes list, so both lists answer a
      right-click the same way
- [ ] 2.3 Section-header menu: new change, refresh

## 3. Verify

- [ ] 3.1 Right-click an unselected row: selection moves, then every item acts on
      that change
- [ ] 3.2 Archive a complete change: row leaves the section, counts drop
- [ ] 3.3 Validate a change with a known problem: the message names it plainly
- [ ] 3.4 Repo without `openspec/` shows no Specs section and no menu
- [ ] 3.5 Manual check in a native window
