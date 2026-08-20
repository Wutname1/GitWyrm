# Agent Desk Spec Delta

## ADDED Requirements

### Requirement: OpenSpec sources remain file-backed

Agent Desk SHALL read and update OpenSpec through repository files and existing writers.
Session records SHALL NOT become a second source of truth for task/spec state.

#### Scenario: Task completion

- WHEN an accepted run completes an OpenSpec task
- THEN the task checkbox is written to tasks.md and every progress surface refreshes

### Requirement: Exact task identity is preserved

A session started from one task SHALL keep that task's parsed index and text even when it
is not the next open task or its display number is duplicated.

#### Scenario: Non-next task

- WHEN the user starts task 7 while task 3 is still open
- THEN context, execution, completion, and source banner all target task 7

### Requirement: Plans trace to written requirements

Plan graph nodes SHALL be able to link to the OpenSpec task and requirement/scenario that
caused them, and SHALL become stale when those source files change before Start.

#### Scenario: Requirement changes

- WHEN a spec delta changes after a graph is drafted
- THEN Start pauses until the user refreshes or revises the plan

### Requirement: AI remains optional

OpenSpec editing, copying, and external handoff SHALL remain available without a usable AI
provider.

#### Scenario: No provider

- WHEN no provider is configured
- THEN the OpenSpec source detail still supports manual edits and handoff actions
