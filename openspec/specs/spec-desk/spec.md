# spec-desk Specification

## Purpose
TBD - created by archiving change add-spec-desk-detail. Update Purpose after archive.
## Requirements
### Requirement: Change header

The Desk detail SHALL show a breadcrumb (`openspec / changes / <id>`), the change's
human title, its one-line goal, and its status pill above the tab bar. The header SHALL
always describe the change whose content is displayed below it.

#### Scenario: Header honesty

- WHEN any tab (including a future AI tab) shows content for change X
- THEN the header above it shows change X, never a different selected change

### Requirement: Overview tab

The Overview tab SHALL show: a progress card (ring with percent, "X of Y done", a
signal line, and a state-appropriate hint), a latest-activity card with the three most
recent history entries, and a change-package grid of Proposal / Spec deltas / Design /
Tasks cards marking what exists.

#### Scenario: Signal grammar

- WHEN one task remains
- THEN the signal reads "1 task remains" (and "3 tasks remain" for three)

#### Scenario: Hints per state

- WHEN a change has no tasks
- THEN the hint says to add tasks to tasks.md (plain markdown)
- AND WHEN all tasks are done, the hint points at the spec check and archive

### Requirement: Proposal tab

The Proposal tab SHALL render the change's Why, What Changes, and Impact sections from
proposal.md.

#### Scenario: Read the pitch

- WHEN the user opens the Proposal tab
- THEN they see the three sections with the file's text, readable without opening an editor

### Requirement: Spec deltas tab

The Spec deltas tab SHALL show one card per delta with its kind (ADDED / MODIFIED /
REMOVED), target spec file, and the requirement and scenario text. The tab label SHALL
carry the delta count. An empty state SHALL explain deltas are written during the
proposal step.

#### Scenario: Kinds are visually distinct

- WHEN a change has one ADDED and one MODIFIED delta
- THEN the tab badge shows 2 and each card's kind is color-badged

### Requirement: History tab

The History tab SHALL list the change's history newest-first with icon, plain-language
text, and "when · who". Every AI-performed action SHALL be attributed in the form
"with <provider> · reviewed by you" - the human is always the author of record.

#### Scenario: Attribution

- WHEN a delta was drafted by the AI and accepted by the user
- THEN history shows "Spec delta drafted with <provider> · reviewed by you"

### Requirement: Interactive task list

The Desk SHALL render tasks.md as a grouped checklist. Clicking a checkbox SHALL write
the toggle to tasks.md. The first open task SHALL be highlighted "Ready now"; done
tasks SHALL be struck through with a Done label; other open tasks show "Later". Each
open task SHALL expose a hover action to hand just that task off.

#### Scenario: Tick writes the file

- WHEN the user ticks a task
- THEN tasks.md is updated on disk and every progress display updates
- AND a confirmation notes the write went to tasks.md

#### Scenario: Ready now

- WHEN a change has open tasks
- THEN exactly the first open one is highlighted as Ready now

#### Scenario: Empty tasks

- WHEN a change has no tasks
- THEN the list area says "No tasks yet - add them in tasks.md (plain markdown)."

### Requirement: Inert content rendering

All markdown sourced from openspec files or user input SHALL render inert - no HTML or
script inside file content may execute or alter the page structure.

#### Scenario: Angle brackets

- WHEN a proposal contains a literal `<` (for example "diff < 500 lines")
- THEN it renders as text and the rest of the page is unaffected

### Requirement: Popout window

The Spec Desk SHALL be a separate OS window per repository. The main window SHALL
stay fully usable while the Desk is open. Opening the Desk when one is already
open SHALL focus it. Remembering size and position per repository is specified in
`app-windows`.

#### Scenario: Second monitor

- WHEN the user opens the Spec Desk and moves it to another monitor
- THEN both windows work simultaneously, and the Desk reopens where it was left

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

