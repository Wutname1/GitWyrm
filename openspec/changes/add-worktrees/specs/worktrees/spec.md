# worktrees Spec Delta

## ADDED Requirements

### Requirement: Worktrees are visible where branches are

The left panel SHALL list the repository's worktrees alongside Branches and
Submodules, showing each worktree's folder name, the branch it has checked out, and
which one is currently open. A repository with no extra worktrees SHALL still show
the section with a way to add one, so the feature is discoverable before it is needed.

#### Scenario: Seeing what exists

- WHEN a repository has two worktrees
- THEN both are listed with their branches, and the open one is marked

### Requirement: Adding a worktree explains where files will go

Adding a worktree SHALL ask for a branch (existing or new) and a folder, and SHALL
suggest a default location outside the repository. The dialog SHALL state in plain
words that a new folder of working files is being created on disk.

#### Scenario: Default location is safe

- WHEN the add dialog opens
- THEN the suggested folder is outside the repository working tree, so the new
  checkout can never be committed into the repository by accident

### Requirement: The active worktree is never ambiguous

When a worktree is open, the status bar SHALL name it. Any action that changes files
SHALL act on the worktree the user is looking at.

#### Scenario: Two checkouts open

- WHEN the user has the main checkout and one worktree open and commits in one
- THEN the commit lands in the checkout named in that window's status bar, and the
  other is untouched

### Requirement: A branch checked out elsewhere is explained, not errored

When a branch is already checked out in another worktree, GitWyrm SHALL explain that
in plain language and offer to open that worktree, instead of surfacing git's raw
error text.

#### Scenario: Double checkout attempt

- WHEN the user tries to check out a branch that another worktree holds
- THEN GitWyrm says which worktree has it and offers to open that one

### Requirement: Removing a worktree says what is lost

Removing a worktree SHALL use a plain-language confirm that states whether the
worktree has uncommitted changes and what happens to them. It SHALL NOT require
typing a name to confirm.

#### Scenario: Uncommitted work present

- WHEN the user removes a worktree with uncommitted changes
- THEN the confirm names the number of changed files before the user decides

### Requirement: Worktrees broken outside GitWyrm can be repaired

When a worktree's folder has been moved or deleted outside GitWyrm, the section SHALL
show it as broken and offer to repair or prune it, rather than failing silently.

#### Scenario: Folder deleted in Explorer

- WHEN a worktree folder is deleted outside the app
- THEN it appears as broken with a prune action that cleans up the reference

### Requirement: A run can work in its own worktree

Starting a Spec Desk run SHALL offer running the task in its own worktree. The run
works in that checkout, the user keeps working in theirs, and the result SHALL be
reviewed as a diff before it reaches the user's branch. Discarding such a run SHALL
delete its worktree.

#### Scenario: Working during a run

- WHEN a task runs in its own worktree
- THEN the user's working tree is unchanged while the run is in progress

#### Scenario: Discarding an isolated run

- WHEN the user discards a run that worked in its own worktree
- THEN the worktree folder is removed and the user's branch is untouched
