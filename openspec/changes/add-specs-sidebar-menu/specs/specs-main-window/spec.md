# specs-main-window Spec Delta

## ADDED Requirements

### Requirement: Spec row context menu

Each row in the Specs sidebar section SHALL offer a context menu on right-click,
consistent with the branch, stash, and submodule sections. Right-clicking a row
SHALL first make that change the selected one, so the menu and every other spec
surface describe the same change.

The menu SHALL offer: open the Spec Desk at that change, copy the next-task
handoff, copy the change id, and validate the change. Each item SHALL act on the
right-clicked row, not on whatever was previously selected.

#### Scenario: Right-click selects

- WHEN the user right-clicks a change that is not selected
- THEN that change becomes selected everywhere before the menu opens
- AND the menu's actions apply to it

#### Scenario: Open from the menu

- WHEN the user picks "Open in Spec Desk"
- THEN the Desk opens (or focuses) with that change selected

#### Scenario: Handoff from the menu

- WHEN the user picks "Copy next-task handoff" on a change with an open task
- THEN the handoff text is on the clipboard and a confirmation says so
- AND the item is absent for a change whose tasks are all done

#### Scenario: Validation result is reported

- WHEN the user picks "Validate"
- THEN the result is shown as a pass or a plain-language list of problems

### Requirement: Archiving from the sidebar

The context menu SHALL offer archiving only when every task in the change is done,
and SHALL confirm before archiving. The item SHALL be absent rather than disabled
while any task is open, matching how the app hides actions that cannot apply.

#### Scenario: Complete change

- WHEN every task is done and the user picks "Archive"
- THEN a confirmation names the change and says it moves out of the active list
- AND accepting moves it, after which the row leaves the Specs section

#### Scenario: Incomplete change

- WHEN a change has open tasks
- THEN the menu shows no archive item at all

#### Scenario: Confirming without typing

- WHEN the archive confirmation appears
- THEN it is accepted with a single click, never by typing a word or phrase

### Requirement: Specs section header menu

The Specs section header SHALL offer a context menu for actions that are about the
set of changes rather than one of them: start a new change, and refresh from disk.

#### Scenario: New change

- WHEN the user picks "New change" from the header menu
- THEN the new-change flow opens, the same one the Desk's "＋ New" button opens

#### Scenario: Refresh

- WHEN the user picks "Refresh"
- THEN every spec surface re-reads `openspec/` and reflects any outside edits
