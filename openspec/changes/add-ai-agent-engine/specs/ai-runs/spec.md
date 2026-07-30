# ai-runs Spec Delta

## ADDED Requirements

### Requirement: The agent is GitWyrm's own

The run engine SHALL be part of GitWyrm. Running a task SHALL NOT require another agent
application to be installed, and the guardrails SHALL be enforced in GitWyrm's own
process rather than delegated to another tool.

#### Scenario: No other agent app installed

- WHEN the user has no third-party agent application on their machine
- THEN running a task still works, using the provider CLI they already signed in to

#### Scenario: Guardrails are ours

- WHEN the engine is asked to do something a guardrail forbids
- THEN GitWyrm refuses or gates it, regardless of what the underlying provider would allow

### Requirement: Model access through the provider's own CLI

The engine SHALL reach models through the provider's command-line tool - Claude Code for
Anthropic, the GitHub Copilot CLI for Copilot - so the user's existing sign-in is reused
rather than asking for another API key. A missing CLI SHALL be reported in plain language
naming what to install.

#### Scenario: Reusing an existing sign-in

- WHEN the user is already signed in to their provider's CLI
- THEN GitWyrm runs tasks with it without asking for a key

#### Scenario: CLI absent

- WHEN the provider's CLI is not installed
- THEN the Desk shows the reconnect state, says which tool to install, and the
  copy-handoff path still works

### Requirement: A bounded tool set

The loop SHALL be able to read a file, edit a file, list a directory, and run one of the
project's own checks - and nothing else. Every path SHALL resolve inside the repository;
a path outside it SHALL be refused.

#### Scenario: Outside the repository

- WHEN the engine attempts to read or write a path outside the repository
- THEN the attempt is refused and the run reports it as a step that could not be taken

#### Scenario: No arbitrary commands

- WHEN the engine wants to run something that is not one of the project's own checks
- THEN it is raised as a gate rather than executed

### Requirement: A run cannot spin forever

Every run SHALL end: when the task's done-means checks pass, when its turn budget is
spent, or when the user stops it. A run that exhausts its budget SHALL report that as its
cause rather than appearing to still be working.

#### Scenario: Budget spent

- WHEN the loop reaches its turn budget without the checks passing
- THEN the run ends as "didn't finish", naming the budget as the reason

### Requirement: Stop is prompt and leaves a clean tree

Stopping SHALL cancel the engine promptly, including part-way through a turn, and SHALL
leave the working tree in a state the console's keep-or-undo choices can act on - never a
half-written file.

#### Scenario: Stop mid-turn

- WHEN the user stops while the engine is editing
- THEN no partially-written file is left behind, and the user's own uncommitted work is
  exactly as it was before the run
