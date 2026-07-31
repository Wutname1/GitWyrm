# spec-desk Specification Delta

## ADDED Requirements

### Requirement: Editing a spec file

Every file in a change package - proposal.md, tasks.md, design.md, and each spec
delta - SHALL be editable from the tab that displays it, without leaving the Desk.
Editing SHALL be an explicit mode entered by a control on that tab; a tab SHALL
render read-only until the user asks to edit it.

The editor SHALL show the file's raw markdown, not the rendered view, so what is
saved is what was typed.

#### Scenario: Reading does not risk changing

- WHEN the user opens the Proposal tab and types
- THEN nothing is altered, because the tab is not in edit mode

#### Scenario: Every package file is reachable

- WHEN a change has a design.md and two deltas
- THEN each one can be opened for editing from the tab that shows it

### Requirement: Unsaved work is never lost silently

Edits SHALL be held per file while the Desk is open, and SHALL survive switching
tabs and selecting another change and returning. A file with unsaved edits SHALL
be marked as such where it is listed.

Any action that would discard unsaved edits SHALL confirm first, naming the file.

#### Scenario: Tab switching keeps the draft

- WHEN the user edits proposal.md, switches to Spec deltas, and switches back
- THEN their unfinished text is still in the editor, still marked unsaved

#### Scenario: Discarding asks first

- WHEN the user leaves edit mode on a file with unsaved edits
- THEN they are asked before the text is thrown away, and the file is named

### Requirement: Saving writes the file

Saving SHALL write the editor's text to the file on disk and return the tab to
its read-only rendering, showing the saved result. A save that fails SHALL say so
in plain language and SHALL keep the user's text in the editor.

Writes SHALL be confined to the change package directory. A path resolving outside
`openspec/changes/<id>/` SHALL be refused.

#### Scenario: Saved work shows up

- WHEN the user saves an edited proposal
- THEN the Proposal tab renders the new text, and the change list reflects any
  changed title or goal

#### Scenario: A failed write does not eat the text

- WHEN the file cannot be written (read-only, gone, permissions)
- THEN the message explains what happened and the typed text is still there
