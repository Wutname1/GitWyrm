# spec-desk Spec Delta

## ADDED Requirements

### Requirement: Popout window

The Spec Desk SHALL be a separate OS window per repository. The main window SHALL stay
fully usable while the Desk is open, and opening the Desk when one is already open SHALL
focus it. Remembering size and position is specified separately in `app-windows`.

#### Scenario: Second monitor

- WHEN the user opens the Spec Desk and moves it to another monitor
- THEN both windows work simultaneously and stay independently usable

#### Scenario: Keep on top

- WHEN the user toggles Keep on top
- THEN the Desk floats above other windows until toggled off, and the toggle state is visible

#### Scenario: Show main window

- WHEN the user clicks Show main window in the Desk titlebar
- THEN the main window is focused and the Desk stays open

### Requirement: Changes list

The Desk SHALL list every active change with its id, last-updated time, status pill
(In build / Needs review / Draft / Ready to archive), and a progress bar. Selecting a
row SHALL select that change in both windows.

#### Scenario: Row anatomy

- WHEN the list renders
- THEN each row shows id (monospace), "updated <time>", a colored status pill, and a progress bar
- AND the selected row is visibly marked

#### Scenario: Cross-window selection

- WHEN the user selects a change in the Desk
- THEN the main window's sidebar row and spec card switch to it, and the reverse also holds

### Requirement: List filters

The changes list SHALL offer Active, Needs review, and Mine filters. An empty filter
result SHALL say so rather than showing a blank column.

#### Scenario: Needs review

- WHEN the user picks Needs review
- THEN only changes in the needs-review state are listed
- AND if there are none, a short "nothing matches" message shows

### Requirement: Archive access

The Desk SHALL show an archive link with the count of completed changes, opening a
searchable list of archived changes with their final proposals and merged deltas.

#### Scenario: Count

- WHEN 17 changes have been archived
- THEN the link reads "Archive · 17 completed changes" and updates when archiving

### Requirement: Desk status bar

The Desk status bar SHALL show the selected change id and confirm that tasks.md is
watched and edits save instantly.

#### Scenario: Reassurance

- WHEN the Desk is open on a change
- THEN the status bar reads "<change-id> · tasks.md watched · saved instantly"

### Requirement: One source of truth across windows

Both windows SHALL render the same parsed openspec state. Any mutation (task tick,
create, archive) from either window or from disk SHALL be reflected in both windows
within one second.

#### Scenario: Tick in Desk, see in main

- WHEN a task is ticked in the Desk
- THEN the main window's sidebar bar, spec card ring, and graph tip chip update together
