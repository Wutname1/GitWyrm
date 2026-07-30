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

### Requirement: Three transports, one interface

The engine SHALL reach models by any of: driving a provider's own installed CLI as a
subprocess, a provider's documented API with an API key the user supplies, or an
OpenAI-compatible endpoint. All SHALL sit behind one interface, and no provider-specific
behavior SHALL reach the console or any other UI.

#### Scenario: Provider CLI present

- WHEN the default provider ships a CLI that GitWyrm can drive, and it reports itself usable
- THEN the engine runs tasks through it, and never handles a credential itself

#### Scenario: Copilot switched off by an administrator

- WHEN a user's organization or enterprise has disabled Copilot CLI
- THEN the run reports that plainly as something an administrator controls, rather than
  reading as a GitWyrm fault

#### Scenario: A CLI's integration surface changes

- WHEN a provider CLI's integration protocol changes or becomes unavailable, as a
  preview-status protocol may
- THEN the failure is contained to that one transport: the run reports that provider as
  currently unusable and the copy-handoff path still works, rather than the engine failing
  as a whole

#### Scenario: API key present

- WHEN the default provider is configured with an API key
- THEN the engine runs tasks through that provider's documented API

#### Scenario: A local or self-hosted model

- WHEN the default provider is an OpenAI-compatible endpoint - a local opencode server,
  Ollama, LM Studio, or similar
- THEN the engine runs tasks through it with no special-casing

### Requirement: Never another application's credentials

GitWyrm SHALL NOT read, write, or inspect credential files or configuration belonging to
another application, and SHALL NOT reuse a credential issued to another application. Where
a CLI is driven, that CLI authenticates itself.

This is not a preference. Reading a coding CLI's stored credentials and calling the API
directly is the pattern that drew legal action against a comparable project; the sourcing
is in this change's design.md.

#### Scenario: Another tool's credentials on disk

- WHEN a provider's own CLI has credentials stored on the machine
- THEN GitWyrm neither reads them nor relies on them, whatever convenience that would offer

#### Scenario: Credentials present but unusable

- WHEN a provider reports itself not usable - not logged in, or lacking a needed scope -
  despite credentials existing
- THEN GitWyrm believes that answer rather than starting a run that would fail later

### Requirement: Anthropic access is by API key only

The engine SHALL NOT drive an Anthropic CLI as a subprocess. Anthropic prohibits
third-party products routing requests through Free, Pro, or Max plan credentials on their
users' behalf, and is the one provider known to have enforced it. Anthropic runs require an
API key until that position changes or written approval is obtained.

#### Scenario: Anthropic default without a key

- WHEN the default provider is Anthropic and no API key is configured
- THEN GitWyrm says plainly that a key is needed and where to get one, does not reach for a
  locally-installed Anthropic CLI, and leaves the copy-handoff path fully available

### Requirement: The engine uses the user's default provider

The engine SHALL resolve which provider and model to use from the user's default in AI
settings, through the same shared path every other AI feature uses. It SHALL NOT carry its
own provider selection.

#### Scenario: One answer everywhere

- WHEN a run starts
- THEN it uses the same provider and model that commit-message generation would use

#### Scenario: Default cannot run

- WHEN the default provider has no usable transport
- THEN the message names what is missing and what to do, rather than reading as a fault or a
  failed run

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

Every run SHALL end: when the targeted task's checkbox is ticked, when its turn budget is
spent, or when the user stops it. A run that exhausts its budget SHALL report that as its
cause rather than appearing to still be working.

The budget SHALL be counted in turns - complete plan/act/observe cycles - rather than
elapsed time, so a task behaves the same way regardless of how fast the chosen provider
is. It SHALL default to 12 and SHALL be adjustable in settings.

#### Scenario: Budget spent

- WHEN the loop reaches its turn budget without the task's checkbox ticked
- THEN the run ends as "didn't finish", naming the budget as the reason

#### Scenario: Budget raised

- WHEN the user raises the turn budget in settings
- THEN later runs use the new value, and a run already under way keeps the budget it
  started with

### Requirement: Done means the task's checkbox is ticked

A run targets one task, and SHALL treat that task's checkbox in `tasks.md` becoming ticked
as the signal to stop. The engine SHALL NOT invent a separate notion of completion, so
what ends the loop is the same thing the user sees in the Desk.

A run ending SHALL NOT by itself apply the work. The console's keep-or-undo choice remains
the gate, so a checkbox ticked without the work being done is caught by review rather than
silently accepted.

#### Scenario: Task completed

- WHEN the targeted task's checkbox is ticked during a run
- THEN the loop stops and the run reports as finished, with its changes still awaiting the
  user's keep-or-undo choice

#### Scenario: No check to run

- WHEN the targeted task is documentation or spec text, with no project check that could
  prove it
- THEN the run can still complete, because the checkbox and not a passing check is what
  defines done

### Requirement: Stop is prompt and leaves a clean tree

Stopping SHALL cancel the engine promptly, including part-way through a turn, and SHALL
leave the working tree in a state the console's keep-or-undo choices can act on - never a
half-written file.

#### Scenario: Stop mid-turn

- WHEN the user stops while the engine is editing
- THEN no partially-written file is left behind, and the user's own uncommitted work is
  exactly as it was before the run
