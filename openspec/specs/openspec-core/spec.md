# openspec-core Specification

## Purpose
TBD - created by archiving change add-openspec-foundation. Update Purpose after archive.
## Requirements
### Requirement: OpenSpec detection

GitWyrm SHALL detect an `openspec/` folder at the repository root. When it is absent,
no Specs UI appears anywhere and no errors are shown.

#### Scenario: Repo without openspec

- WHEN a repository without an `openspec/` folder is opened
- THEN no Specs sidebar section, spec card, Desk entry point, or status-bar segment is shown
- AND nothing is logged as an error

#### Scenario: Folder appears later

- WHEN the user (or a tool) creates `openspec/` with at least one change while the repo is open
- THEN the Specs surfaces appear without restarting or reopening the repository

### Requirement: Change parsing

GitWyrm SHALL parse each `openspec/changes/<id>/` folder into a typed change: proposal
(Why / What Changes / Impact), tasks (groups and checkboxes from tasks.md), design.md
presence, and spec deltas (ADDED / MODIFIED / REMOVED requirements with scenarios).

#### Scenario: Well-formed change

- WHEN a change folder contains proposal.md, tasks.md, and delta files
- THEN all sections are available to the UI with their text intact

#### Scenario: Malformed file degrades gracefully

- WHEN a file in a change folder does not match the expected structure
- THEN the change still lists, unparseable sections render as raw markdown
- AND GitWyrm never crashes or blocks other changes from loading

### Requirement: Progress comes from tasks.md

A change's progress SHALL be computed from tasks.md checkboxes: done count, total
count, and percent. A change with no tasks reports as a draft.

#### Scenario: Counting

- WHEN tasks.md has 10 checkboxes and 7 are `- [x]`
- THEN the change reports 7 of 10 done and 70 percent

#### Scenario: No tasks

- WHEN tasks.md is missing or has no checkboxes
- THEN the change reports as a draft, not as 0 percent of nothing

### Requirement: File watching

GitWyrm SHALL watch `openspec/` and refresh parsed state within one second of any file
change, regardless of what tool made the edit.

#### Scenario: External edit

- WHEN an agent or editor marks a task done in tasks.md outside GitWyrm
- THEN every GitWyrm surface showing that change updates within one second, with no manual refresh

### Requirement: Task write-back

Toggling a task in GitWyrm SHALL write exactly one checkbox change (`- [ ]` to `- [x]`
or back) to tasks.md, preserving every other byte of the file.

#### Scenario: Toggle preserves the file

- WHEN the user ticks a task in GitWyrm
- THEN tasks.md on disk differs only in that one checkbox
- AND formatting, ordering, comments, and trailing whitespace are untouched

### Requirement: CLI integration

GitWyrm SHALL use the `openspec` CLI for validate and archive when it is installed, and
SHALL report a typed CLI-missing outcome (with a plain-language install hint) when it is
not. Read-only viewing SHALL never require the CLI.

#### Scenario: CLI present

- WHEN the user runs a spec check and the CLI is installed
- THEN GitWyrm runs `openspec validate <id>` and shows its result

#### Scenario: CLI absent

- WHEN the user runs a spec check and the CLI is not installed
- THEN GitWyrm explains the check needs the OpenSpec tool and how to get it
- AND all viewing, progress, and task ticking still work

#### Scenario: CLI installed while GitWyrm is open

- WHEN the user installs the CLI after reading that hint and asks to check again
- THEN GitWyrm re-probes rather than repeating the cached answer, and runs the
  check without needing a restart

### Requirement: Typed command surface

All OpenSpec operations SHALL be exposed as typed commands through the project's
specta-generated bindings, matching how every other backend command ships.

#### Scenario: Bindings regenerated

- WHEN the openspec commands are added
- THEN `src/lib/bindings.ts` includes their typed signatures via the normal regeneration script

