# spec-desk-actions Spec Delta

## ADDED Requirements

### Requirement: Handoff composition

GitWyrm SHALL compose a paste-ready handoff for a task containing: the change id, the
task number and text, the files to read first (proposal.md, design.md, each spec delta
with its kind), and the rules (do only this task, mark it done in tasks.md, run
`openspec validate <id>`, keep unrelated work intact). When all tasks are done it SHALL
compose a review handoff instead. The exact text SHALL be previewable before copying.

#### Scenario: Copy next task

- WHEN the user copies the next-task handoff
- THEN the clipboard holds the composed text and a confirmation says where to paste it

#### Scenario: Preview matches clipboard

- WHEN the preview shows the handoff
- THEN copying places exactly that text on the clipboard

#### Scenario: Per-task handoff

- WHEN the user uses a task row's hover copy action
- THEN the handoff is scoped to that task, not the next one

### Requirement: External tool launchers

The rail SHALL offer Open in opencode (starts opencode in the repository with the
handoff as its opening message) and Open in VS Code (opens the repository with the
handoff on the clipboard). These SHALL remain one click away regardless of AI
configuration.

Open in opencode SHALL be offered only when opencode can be launched, and SHALL say
so when it cannot, rather than opening a terminal the user must finish by hand.

#### Scenario: opencode

- WHEN the user clicks Open in opencode
- THEN opencode starts in a terminal at the repo with the handoff already in the
  conversation, requiring no paste

#### Scenario: opencode is not installed

- WHEN opencode cannot be found on the machine
- THEN the button is disabled and names the fix, and the other handoff actions
  continue to work

### Requirement: Editable handoff templates

Handoff templates SHALL be editable in Settings → Specs with `{change}`, `{task}`, and
`{deltas}` variables, so teams can tune the house style once.

#### Scenario: Custom template

- WHEN a team edits the template to add a coding-standards line
- THEN every subsequent handoff includes it

### Requirement: Spec check with visible results

Run spec check SHALL execute `openspec validate` for the selected change and show the
result inline in the rail - pass states what was checked; a warning names the problem
in plain language and what to do about it. Results SHALL persist until the user
switches changes and SHALL NOT be delivered by toast alone.

#### Scenario: Missing deltas

- WHEN a change with no spec deltas is checked
- THEN an inline warning explains a change needs at least one requirement before it can be archived

#### Scenario: Pass

- WHEN a well-formed change is checked
- THEN the inline result says the check passed and what was validated

### Requirement: Archive flow

Archiving SHALL be blocked, with the number of remaining tasks stated, until every task
is done and at least one delta exists. Archiving SHALL use a plain-language confirm
(never type-to-confirm), merge the change's deltas into the specs library, move the
change to the archive, and update lists, counts, and selection in both windows.

#### Scenario: Blocked

- WHEN 3 tasks are open and the user clicks archive
- THEN nothing is archived and the message says 3 tasks remain

#### Scenario: Archive completes

- WHEN a finished change is archived and confirmed
- THEN it leaves the active list, the archive count increments, its deltas land in specs/,
  and the selection moves to another change

### Requirement: New blank change

The Desk SHALL create a new change from a description and a user-chosen kebab-case
folder name. The name is never chosen silently by the system; duplicate names are
rejected while the name field is still editable. Creation writes template proposal.md
and tasks.md, and the new change appears selected in both windows.

#### Scenario: Scaffold

- WHEN the user creates `warn-before-stash-delete` with a one-line description
- THEN `openspec/changes/warn-before-stash-delete/` exists with template files
- AND the change is selected in the Desk and visible in the main-window sidebar

#### Scenario: Duplicate name

- WHEN the chosen name already exists
- THEN creation is refused with a plain message and the name field remains editable
