# Agent Desk Spec Delta

## ADDED Requirements

### Requirement: Issue Fix starts visibly and in isolation

An issue SHALL offer Fix with AI. The clicked source SHALL show Starting and Agent Desk
SHALL show a Preparing session before provider/host/worktree preparation finishes. Fix
SHALL edit only an isolated worktree.

#### Scenario: Slow preparation

- WHEN host and provider preparation each take several seconds
- THEN the issue and new session acknowledge the action immediately

#### Scenario: Worktree failure

- WHEN isolation cannot be created
- THEN Fix does not edit the user's checkout and the session offers a clear retry path

### Requirement: Pull-request reading intents stay read-only

Pull requests SHALL offer Review with AI and Summarize with AI. These intents SHALL NOT
create a worktree, edit files, commit, push, or post to the host.

#### Scenario: Review completes

- WHEN Review finishes
- THEN its findings exist in the session and the repository working trees are unchanged

### Requirement: The launch source is cached and refreshed separately

Kickoff SHALL save already-loaded source data before background enrichment and SHALL show
when live data later differs.

#### Scenario: Host becomes unavailable

- WHEN the host cannot be reached after kickoff
- THEN the session still shows the cached source and explains live refresh failed

### Requirement: Duplicate active work is intentional

Starting the same active source/intent SHALL focus the existing session or ask explicitly
before creating another execution.

#### Scenario: Double Fix

- WHEN the user invokes Fix twice on the same issue while it is working
- THEN GitWyrm focuses the active session and does not start a second Fix silently
