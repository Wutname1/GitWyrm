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
The selected reading operation SHALL start from the source action without requiring a
second message or Send action.

#### Scenario: Review completes

- WHEN Review finishes
- THEN its findings exist in the session and the repository working trees are unchanged

#### Scenario: Remembered provider permission

- WHEN the provider has a global or remembered rule that normally allows file writes
- THEN Review and Summarize still launch with writes disabled and leave every checkout byte-identical

### Requirement: Every explicit source action starts its operation

Fix, Plan, Explain, Review, and Summarize SHALL create or focus the session and start the
selected operation. Read-only authority SHALL limit capabilities, not delay execution.

#### Scenario: Summarize from a pull request

- WHEN the user chooses Summarize with AI
- THEN the session appears immediately and the summarization run starts without another click

### Requirement: Kickoff choices survive into execution

The selected intent, mode, team shape, and provider override SHALL be stored with the
session and used for its first execution. When no override is selected, GitWyrm SHALL use
the configured default provider. An unavailable or unsupported provider SHALL fail visibly
instead of silently switching providers.

#### Scenario: Provider override

- WHEN the user chooses Fix with a specific supported provider
- THEN the first Fix execution uses that provider and displays it in execution details

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

#### Scenario: Concurrent starts race

- WHEN two start requests race and one loses after preparing an isolated worktree
- THEN the losing request removes its unused worktree unless it contains unique work and focuses the winner
