# Change: Right-click actions on sidebar spec rows

## Why

Every other thing in the left panel answers a right-click - branches, stashes,
submodules, remotes all have a context menu. Spec rows are the exception: the only
thing a row does is select. So the actions that already exist behind those rows
(validate, archive, copy a handoff, open the Desk) are reachable only by first
selecting the change and then hunting for the action somewhere else, which is a
detour the rest of the panel does not ask for.

Nothing here is a new capability. Every item maps to a command that already ships;
this change is about where the user can reach them.

## What Changes

- Context menu on each Specs row, opened by right-click, matching the panel's other
  sections in shape and wording
- Items: open the Desk at that change, copy the next-task handoff, copy the change id,
  validate, archive (when every task is done)
- Right-clicking a row selects it first, so the menu and the rest of the app always
  describe the same change
- Archive stays behind a confirm and is absent - not greyed - while tasks are open
- Section-header right-click for change-wide actions: new change, refresh

## Impact

- Affected specs: `specs-main-window`
- Affected code: `src/components/domain/left-panel/SpecsSection.tsx`, a new
  `SpecContextMenu` shared with the Desk's changes list
- Depends on: `add-specs-sidebar-and-card` (the rows), `add-spec-desk-window` (the
  Desk entry point). Both shipped.
