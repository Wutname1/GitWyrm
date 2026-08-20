# Agent Desk Spec Delta

## ADDED Requirements

### Requirement: External history import is read-only and attributed

Agent Desk SHALL read supported external client sessions without modifying their history.
Imported messages SHALL retain visible client/session provenance.

#### Scenario: Import

- WHEN a Codex session is imported
- THEN its messages appear in a Codex-labeled segment and Codex files are unchanged

### Requirement: Adapters fail independently

One missing, corrupt, slow, or unsupported client SHALL not block native Agent Desk
sessions or other adapters.

#### Scenario: Corrupt OpenCode session

- WHEN one OpenCode session cannot be parsed
- THEN other OpenCode sessions and all native sessions remain usable

### Requirement: Continuation is honest

Continue here SHALL create a native segment with preserved provenance. Continue externally
SHALL appear only when a supported launch exists and SHALL not claim context transfer that
did not occur.

#### Scenario: Open only

- WHEN an adapter can open its client but cannot resume a specific session
- THEN the action says Open client rather than Continue session

### Requirement: Import is incremental and deduplicated

Repeated scans SHALL not duplicate external sessions/messages and SHALL retain unresolved
project paths visibly.

#### Scenario: Re-scan

- WHEN a previously imported session has one new message
- THEN only that message is added to the existing imported segment
