# Agent Desk Spec Delta

## ADDED Requirements

### Requirement: Results are reviewed through repository truth

Agent Desk SHALL link results to GitWyrm's existing diff/check/commit data rather than
persist copied diff text as a second source of truth.

#### Scenario: Helper diff

- WHEN the user chooses View diff on a helper
- THEN GitWyrm opens that helper's current worktree changes in the normal diff view

#### Scenario: Review a completed graph

- WHEN a graph completes
- THEN its primary result reads the lead integration worktree and links to every helper-scoped result

### Requirement: Finished does not mean committed

An agent reporting completion SHALL enter review. Keep, revise, commit, or discard SHALL
remain intentional user actions.

#### Scenario: Lead finishes

- WHEN the lead says the work is complete
- THEN no commit or push occurs until the user chooses the corresponding action

### Requirement: Revision is a real next turn

Request revision SHALL append the user's revision request to the same session and start a
new execution against the retained result worktree. Changing only a result-state label or
showing a toast SHALL NOT satisfy the action.

#### Scenario: User requests a revision

- WHEN the user enters revision guidance and chooses Revise
- THEN the guidance appears in the transcript and a visible new execution starts

### Requirement: Accepted OpenSpec work updates its exact source task

When a result started from an OpenSpec task is accepted, Agent Desk SHALL use the existing
OpenSpec writer to complete that exact task and refresh every surface that displays it.
The action SHALL be unavailable when no exact task can be identified.

#### Scenario: Accept task result

- WHEN the user accepts a result linked to an OpenSpec task
- THEN that task is checked in its source file and the source banner, details, and task list refresh

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

### Requirement: Result recovery runs automatically

On startup, Agent Desk SHALL reconcile persisted executions, results, branches, and marked
worktrees. Missing or orphaned resources SHALL become visible recovery states without
silently deleting provenance or unique work.

#### Scenario: Worktree disappeared while GitWyrm was closed

- WHEN GitWyrm restarts and a retained result worktree is missing
- THEN the session shows an orphaned-result recovery state and keeps its transcript and provenance

### Requirement: Legacy Spec Desk entry points migrate safely

Old URLs and settings SHALL open Agent Desk correctly, and obsolete shell code SHALL NOT
be removed until the new flow passes restart, accessibility, scaling, and performance
checks.

#### Scenario: Old bookmark

- WHEN a legacy Spec Desk URL is opened after migration
- THEN it opens the matching Agent Desk source/session without a duplicate window
