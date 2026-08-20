# Agent Desk Spec Delta

## ADDED Requirements

### Requirement: Results are reviewed through repository truth

Agent Desk SHALL link results to GitWyrm's existing diff/check/commit data rather than
persist copied diff text as a second source of truth.

#### Scenario: Helper diff

- WHEN the user chooses View diff on a helper
- THEN GitWyrm opens that helper's current worktree changes in the normal diff view

### Requirement: Finished does not mean committed

An agent reporting completion SHALL enter review. Keep, revise, commit, or discard SHALL
remain intentional user actions.

#### Scenario: Lead finishes

- WHEN the lead says the work is complete
- THEN no commit or push occurs until the user chooses the corresponding action

### Requirement: Host publication is separate and explicit

Creating/updating a pull request and any required push SHALL be separate explicit actions.
The agent engine SHALL never push or post host comments/reviews implicitly.

#### Scenario: Create PR

- WHEN a kept result is ready
- THEN the user reviews editable PR text and explicitly starts the host/push workflow

### Requirement: Cleanup never destroys the only copy

Agent worktrees SHALL be removed only after work is safely integrated or discarded and
SHALL be kept when hand edits or unique recoverable work remain.

#### Scenario: Hand edit

- WHEN the user edited an agent worktree before cleanup
- THEN cleanup keeps it and offers Open rather than deleting those files

### Requirement: Legacy Spec Desk entry points migrate safely

Old URLs and settings SHALL open Agent Desk correctly, and obsolete shell code SHALL NOT
be removed until the new flow passes restart, accessibility, scaling, and performance
checks.

#### Scenario: Old bookmark

- WHEN a legacy Spec Desk URL is opened after migration
- THEN it opens the matching Agent Desk source/session without a duplicate window
