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

### Requirement: Two transports behind one interface

The engine SHALL reach models either through a provider's documented API (when the user
has a key) or through the provider's command-line tool (Claude Code, the GitHub Copilot
CLI), behind a single interface. Where both are available the API SHALL be preferred: a
CLI is not a public interface and its output can change between releases. The CLI path
exists so a subscription-only user is not shut out. No provider-specific behavior SHALL
reach the console or any other UI.

#### Scenario: Reusing an existing sign-in

- WHEN the user is signed in to their provider's CLI and has no API key
- THEN GitWyrm runs tasks through the CLI without asking for a key

#### Scenario: A key is available

- WHEN the user has an API key for their provider
- THEN the engine uses the documented API rather than the CLI

#### Scenario: Neither available

- WHEN no CLI is installed and no key is set
- THEN the Desk shows the reconnect state, says what to install or add, and the
  copy-handoff path still works

### Requirement: Provider credentials are never touched

GitWyrm SHALL determine whether a provider is usable by asking the provider, and SHALL
NOT read, write, or inspect a provider's credential files or configuration.

#### Scenario: Complete credentials, unusable account

- WHEN a provider CLI holds a complete credential record but reports itself not logged in,
  or lacking a scope it needs
- THEN GitWyrm reports needs-reconnect rather than starting a run that would fail

### Requirement: Discovery survives provider updates

A provider CLI SHALL be located by discovery - PATH, then known install locations - and
accepted on a minimum version rather than a pinned path or exact build, because these
tools update themselves.

#### Scenario: The CLI updates itself

- WHEN a provider CLI updates to a newer build between runs
- THEN GitWyrm keeps working with no change on its side

### Requirement: A turn is never a silent wait

A generation turn takes seconds, not milliseconds - measured at 10 to 20 seconds for a
realistic diff. The engine SHALL stream a turn's output where its transport allows, and
SHALL remain cancellable throughout, so the console always has something to show and Stop
always responds.

#### Scenario: Streaming available

- WHEN the transport can report output progressively
- THEN the engine forwards it as it arrives rather than only at the end of the turn

#### Scenario: Cancel mid-turn

- WHEN the user stops during a turn
- THEN the engine cancels promptly and terminates any child process it started, leaving no
  orphan holding a subscription slot

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
