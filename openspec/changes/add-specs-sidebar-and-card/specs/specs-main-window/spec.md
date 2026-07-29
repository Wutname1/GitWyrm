# specs-main-window Spec Delta

## ADDED Requirements

### Requirement: Specs sidebar section

The main-window sidebar SHALL show a Specs section listing every active change with its
id, a progress bar, and a done count (`n/m`, or `draft` for changes with no tasks). The
section header SHALL show the number of active changes. The section SHALL NOT appear
when the repository has no `openspec/` folder.

#### Scenario: Listing

- WHEN a repo has three active changes
- THEN the Specs section shows three rows and a header count of 3
- AND each row shows the change id in monospace, a progress bar, and its count

#### Scenario: Selection

- WHEN the user clicks a change row
- THEN that change becomes the selected change everywhere (spec card, and the Desk when open)
- AND the row shows a selected state

#### Scenario: Live progress

- WHEN a task is ticked anywhere (GitWyrm, an editor, an agent)
- THEN the row's bar and count update within one second

### Requirement: Spec card

The right panel SHALL show one spec card above the commit form for the selected change:
id, progress ring with percent, "X of Y tasks done", and a preview of the next open
task. The card SHALL offer a copy-next-task-handoff action and an Open Spec Desk action.

#### Scenario: Next task

- WHEN the selected change has open tasks
- THEN the card shows the first open task's text after "Next:"
- AND "Copy next-task handoff" places the composed handoff on the clipboard with a confirmation

#### Scenario: All tasks done

- WHEN every task in the selected change is done
- THEN the card says all tasks are complete and ready to archive
- AND the copy action becomes "Copy review handoff"

#### Scenario: Ambient only

- WHEN the user wants to read the proposal, deltas, or history
- THEN the main window does not render them; the card routes to the Spec Desk instead

### Requirement: Status-bar openspec segment

The status bar SHALL show `openspec · N active` and whether the OpenSpec CLI is
available, only when the repository has an `openspec/` folder.

#### Scenario: Present

- WHEN a repo with 3 active changes and the CLI installed is open
- THEN the status bar shows "openspec · 3 active · CLI v0.9 ✓" (version as detected)

### Requirement: Open Spec Desk entry points

The main window SHALL offer Open Spec Desk from the Specs sidebar footer and from the
spec card. Both SHALL open (or focus, if already open) the Spec Desk window.

#### Scenario: Already open

- WHEN the Desk window is already open on another monitor
- THEN Open Spec Desk focuses it rather than opening a second Desk

### Requirement: Main window stays a git client

Spec surfaces in the main window SHALL be limited to the sidebar section, the spec
card, the status-bar segment, and (per `specs-graph`) commit chips. No spec editing,
proposal reading, or AI run console SHALL live in the main window.

#### Scenario: No scope creep

- WHEN any later change adds spec functionality
- THEN new deep-work UI lands in the Spec Desk, and the main window gains at most status
