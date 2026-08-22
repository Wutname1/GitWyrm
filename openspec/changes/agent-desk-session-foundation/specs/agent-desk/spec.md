# Agent Desk Spec Delta

## ADDED Requirements

### Requirement: Sessions are durable backend-owned records

Agent Desk SHALL persist each session separately in app data with a versioned schema.
Canonical conversation data SHALL NOT exist only in frontend state or `settings.json`.

#### Scenario: Restart

- WHEN a user closes and relaunches GitWyrm
- THEN the session source, messages, execution history, title, and state are restored

#### Scenario: One damaged session

- WHEN one session file cannot be read
- THEN other sessions remain available and the damaged file is not deleted

### Requirement: Source provenance survives live-source changes

Every session SHALL retain a cached snapshot of what started it and, when possible, a
separate live locator. Refreshing SHALL NOT overwrite the launch snapshot.

#### Scenario: Deleted issue

- WHEN the issue that started a session can no longer be loaded
- THEN the original issue title/summary remains visible and the source is marked unavailable

### Requirement: Execution events are ordered and idempotent

Durable session events SHALL carry session, execution, and sequence identifiers. Events
from replaced executions and duplicate sequences SHALL not alter the session.

#### Scenario: Late output

- WHEN an old execution emits after a replacement starts
- THEN that output does not appear in the current transcript

### Requirement: Execution routing is execution-addressed

Every live execution SHALL be linked to exactly one session by execution ID. Repository
identity MAY help find context but SHALL NOT select the destination session for an event.
Linking or unlinking one execution SHALL NOT replace or remove another execution in the
same repository.

#### Scenario: Concurrent sessions in one repository

- WHEN two sessions in the same repository run at the same time
- THEN each event is appended only to the session linked to its execution ID

#### Scenario: One execution finishes

- WHEN one of those executions reaches a terminal state
- THEN only its exact execution link is removed and the other session keeps receiving events

### Requirement: The index is rebuildable

The session list index SHALL contain derived headers only and SHALL be rebuildable from
session files without losing conversation content.

#### Scenario: Invalid index

- WHEN the index is missing or malformed
- THEN Agent Desk rebuilds it and keeps the sessions usable
