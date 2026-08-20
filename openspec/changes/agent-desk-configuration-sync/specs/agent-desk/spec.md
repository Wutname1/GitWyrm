# Agent Desk Spec Delta

## ADDED Requirements

### Requirement: Configuration discovery is separate from writing

Agent Desk SHALL scan supported client skills/connectors without changing them. A write
SHALL require an explicit item, destination, and reviewed preview.

#### Scenario: Scan

- WHEN Agent Setup scans installed clients
- THEN no client configuration file is changed

### Requirement: Copies are granular and conflict-safe

The user SHALL choose configuration per item and destination. Apply SHALL refuse when the
destination changed after preview.

#### Scenario: Concurrent edit

- WHEN another client changes its configuration after preview
- THEN GitWyrm keeps that edit and asks the user to refresh the plan

### Requirement: Every write is recoverable

Before replacing configuration, GitWyrm SHALL create a backup and operation receipt and
SHALL offer Undo subject to conflict checking.

#### Scenario: Undo

- WHEN the copied configuration is still unchanged and the user chooses Undo
- THEN the destination returns byte-for-byte to its previous content

### Requirement: Secrets are not spread silently

Secret-bearing fields SHALL be redacted and SHALL not be copied into insecure plain-text
destinations without an explicit warning and supported safe mechanism.

#### Scenario: Token field

- WHEN a connector includes a token
- THEN inventory/preview/logs do not reveal the token value
