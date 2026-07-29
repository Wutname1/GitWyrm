# Change: Specs in the main window (sidebar section + spec card + status bar)

## Why

The main window is where people already look. Spec status should be ambient there -
visible without competing with git work. Per the v3 mockup, the main window gets
exactly three things: a Specs sidebar section, one spec card by the commit form, and a
status-bar segment. Deep spec work stays out of this window on purpose.

## What Changes

- New "Specs" section in the left sidebar (between Branches and Stashes): one row per
  active change with a progress bar and count
- New spec card at the top of the right panel, above the commit form: selected change,
  progress ring, next task, copy-handoff action, Open Spec Desk action
- Status-bar segment: `openspec · N active · CLI vX ✓`
- "Open Spec Desk" entry points (sidebar footer, card button) that open the popout
  window (window itself ships in `add-spec-desk-window`; until then the action is the
  single integration point they call)

## Impact

- Affected specs: `specs-main-window` (new capability)
- Affected code: `src/components/domain/left-panel/`, `src/components/domain/RightPanel.tsx`,
  `src/components/domain/StatusBar.tsx`
- Depends on: `add-openspec-foundation`
