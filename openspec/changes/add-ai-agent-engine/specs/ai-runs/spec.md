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

### Requirement: Model access is by the user's own API key

The engine SHALL reach models through each provider's documented API, authenticated with
an API key the user supplies. It SHALL NOT route requests through a user's consumer
subscription credentials, and SHALL NOT reuse credentials issued to another application.

This is a terms constraint, not a technical preference. Anthropic prohibits third-party
products routing requests through Free, Pro, or Max plan credentials on their users'
behalf. OpenAI declined four direct requests to confirm that a third-party product may use
ChatGPT sign-in, and its own documentation recommends API keys for programmatic use. The
full sourcing is in this change's design.md.

#### Scenario: A key is present

- WHEN the user has supplied an API key for their provider
- THEN the engine runs tasks through that provider's documented API

#### Scenario: Subscription but no key

- WHEN the user has a consumer subscription to a provider but no API key
- THEN GitWyrm does not attempt to use the subscription, says plainly that a key is needed
  and where to get one, and leaves the copy-handoff path fully available

#### Scenario: No pretending it is broken

- WHEN a provider is unavailable for want of a key
- THEN the message names what is missing rather than reading as a fault or a failed run

### Requirement: Provider credentials are never touched

GitWyrm SHALL NOT read, write, or inspect credential files or configuration belonging to
another application, and SHALL determine a provider's usability by asking that provider
rather than by inferring it from stored files.

#### Scenario: Another tool's credentials on disk

- WHEN a provider's own CLI has credentials stored on the machine
- THEN GitWyrm neither reads nor relies on them

#### Scenario: Credentials present but unusable

- WHEN a provider reports itself not usable - not logged in, or lacking a needed scope -
  despite credentials existing
- THEN GitWyrm believes that answer rather than starting a run that would fail later

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
