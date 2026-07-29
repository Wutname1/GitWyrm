# spec-desk Spec Delta

## ADDED Requirements

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
