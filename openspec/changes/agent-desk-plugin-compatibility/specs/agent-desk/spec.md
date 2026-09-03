# Agent Desk Spec Delta

## ADDED Requirements

### Requirement: A session says what its agent can reach

Before a run starts, the session SHALL show which skills and which connectors the chosen
tool will load, by name. A capability GitWyrm cannot see SHALL be reported as unknown
rather than omitted.

#### Scenario: Capabilities are visible before starting

- WHEN a person opens a chat on a tool with skills and connectors configured
- THEN the session shows what that tool will load

#### Scenario: What cannot be seen is said

- WHEN a tool loads capabilities GitWyrm cannot enumerate
- THEN the session says so rather than showing an empty list that implies none

### Requirement: A capability can be excluded for one session

A person SHALL be able to exclude a skill or connector for one session without editing
the tool's own configuration files. The exclusion SHALL apply to that session's runs and
SHALL NOT change anything on disk outside GitWyrm.

#### Scenario: Turning one off is local

- WHEN a person excludes a connector for a session
- THEN that session's runs do not load it and the tool's own config file is unchanged

#### Scenario: Another session is unaffected

- WHEN one session excludes a skill
- THEN other sessions using the same tool still load it

### Requirement: Exclusion is honest about what it can enforce

Where a tool offers no way to exclude a capability at launch, GitWyrm SHALL say the
exclusion cannot be enforced for that tool rather than showing a control that does
nothing.

#### Scenario: A tool with no exclusion mechanism

- WHEN a tool cannot be told to skip a capability
- THEN the control is absent or disabled with the reason, not shown as working

### Requirement: Carrying a capability reuses the existing copy

Making a skill or connector available to a tool that lacks it SHALL go through the
existing preview, apply and undo path, with its hash gating, backup and refusal to
overwrite silently. It SHALL NOT introduce a second way to write agent configuration.

#### Scenario: Copying into a tool

- WHEN a person makes a skill available to another tool
- THEN it is previewed and applied through the same copy flow, and can be undone

### Requirement: No GitWyrm plugin format

GitWyrm SHALL NOT define its own plugin package, execute plugin code in a runtime of its
own, or require an author to repackage for GitWyrm. Compatibility means running what the
agent tools already load.

#### Scenario: An existing skill needs no changes

- WHEN a skill written for another tool is used through GitWyrm
- THEN it works unchanged, with no GitWyrm-specific manifest or wrapper
