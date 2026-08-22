# ai-runs Spec Delta

> **2026-08-19 correction:** several requirements below describe an in-house
> plan/act/observe loop and three transports. Only one transport shipped (the
> Copilot CLI over ACP), and GitWyrm does not run the loop - the CLI's own
> agent does, over one `session/prompt` per run. Requirements affected are
> marked inline; unmarked requirements still hold as written.

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

### Requirement: The Copilot CLI transport, behind one interface

**(Narrowed from "three transports" - see correction note.)** The engine SHALL reach
models by driving the Copilot CLI's own ACP server as a subprocess. The transport SHALL
sit behind one interface (`ProviderAgent`/`Transport`), so a future transport can be added
without changing the console or any other UI. No provider-specific behavior SHALL reach
the console.

A documented-API transport and an OpenAI-compatible-endpoint transport were planned but
are not implemented. This requirement no longer claims they exist; a future change should
restore the broader requirement only once such a transport is actually built.

#### Scenario: Provider CLI present

- WHEN the Copilot CLI is installed, at or above the version floor, and reports itself
  usable
- THEN the engine runs tasks through it, and never handles a credential itself

#### Scenario: Copilot switched off by an administrator

- WHEN a user's organization or enterprise has disabled Copilot CLI
- THEN the run reports that plainly as something an administrator controls, rather than
  reading as a GitWyrm fault

#### Scenario: A CLI's integration surface changes

- WHEN the Copilot CLI's ACP protocol changes or becomes unavailable, as a preview-status
  protocol may
- THEN the run reports the CLI as currently unusable and the copy-handoff path still
  works, rather than the engine failing as a whole

#### Scenario: No Copilot CLI installed

- WHEN the Copilot CLI is not installed, or is below the version floor
- THEN the run reports plainly that the command-line tool is missing or needs updating,
  and does not attempt any other transport

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

### Requirement: Anthropic has no CLI path

**(Narrowed - see correction note.)** The engine SHALL NOT drive an Anthropic CLI as a
subprocess. Anthropic prohibits third-party products routing requests through Free, Pro,
or Max plan credentials on their users' behalf, and is the one provider known to have
enforced it.

This requirement no longer claims Anthropic runs are available by API key: no API-key
transport is implemented for task runs at all (see "The Copilot CLI transport, behind one
interface" above). Anthropic currently has no run path, the same as every provider other
than Copilot.

#### Scenario: Anthropic as the default provider

- WHEN the default provider is Anthropic
- THEN GitWyrm does not reach for a locally-installed Anthropic CLI, reports that task
  runs are not available for this provider, and leaves the copy-handoff path fully
  available

### Requirement: The engine uses the user's default provider

**(Not implemented - see correction note.)** The engine MUST resolve which
provider and model to use from the user's default in AI settings, through the same shared
path every other AI feature uses, so it never carries its own provider selection. What
ships instead: `run_engine` calls `CliAgent::discover` unconditionally, without reading
the configured default provider. A run today always attempts the Copilot CLI regardless
of what the user set as their default AI provider elsewhere in the app. This is tracked
as a gap, not a design change - the desired behavior is still "one answer everywhere".

#### Scenario: One answer everywhere (target, not current behavior)

- WHEN a run starts
- THEN it SHALL use the same provider and model that commit-message generation would
  use. Today it always attempts the Copilot CLI instead

#### Scenario: Default cannot run

- WHEN the resolved transport has no usable CLI
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

### Requirement: Shell and network access are denied to the driven CLI

**(Narrowed from "a bounded tool set" - see correction note.)** GitWyrm does not
implement its own read/edit/list/check tool set for the loop, because GitWyrm does not
run the loop. What it enforces instead: the Copilot CLI's ACP server SHALL be started
with `shell` and network (`url`) tool kinds denied, so the CLI's own broader tool set
cannot run arbitrary commands or reach the network regardless of what the model asks for.
Denial SHALL take precedence over any allow rule, including a blanket allow-all.

Write-capable isolated runs MAY expose file edits. Read-only operations and Plan proposals
before Start SHALL also launch with the provider's write tool denied. Runtime permission
handling SHALL remain a second boundary but SHALL NOT be the only protection, because a
provider-side allow rule may suppress a permission request.

#### Scenario: The CLI is asked to run a shell command

- WHEN the model asks its own CLI to run a shell command
- THEN the CLI's server denies it, because `shell` is not an available tool kind for the
  session

#### Scenario: The CLI is asked to reach the network

- WHEN the model asks its own CLI to fetch a URL or otherwise reach the network
- THEN the CLI's server denies it, because `url` is not an available tool kind for the
  session

#### Scenario: Read-only run has a remembered allow rule

- WHEN a provider-side rule would normally allow writes without asking GitWyrm
- THEN the provider process still has its write tool denied for that execution

#### Scenario: Plan proposal before Start

- WHEN Plan is drafting a proposal and the user has not chosen Start
- THEN the provider process has write denied and cannot change the checkout

### Requirement: A run ends on the CLI's own stop reason

**(Narrowed from "a run cannot spin forever" - see correction note.)** GitWyrm does not
count turns or enforce a turn budget of its own, because the Copilot CLI runs its own
agent loop and reports its own stop reason (`end_turn`, `max_turn_requests`, `refusal`,
`cancelled`) when a `session/prompt` completes. A run SHALL end when the CLI reports one
of these, or when the user stops it. `max_turn_requests` and `refusal` SHALL be reported
as "didn't finish", naming the CLI's own reason rather than a GitWyrm-counted budget.

There is currently no GitWyrm-side setting to adjust how many turns the CLI's own loop
may take; that ceiling, if any, belongs to the CLI.

#### Scenario: The CLI reports it ran out of turns

- WHEN the Copilot CLI's own turn ceiling is reached without the task's checkbox ticked
- THEN the run ends as "didn't finish", naming the CLI's own stop reason

#### Scenario: The model refuses

- WHEN the CLI's `stopReason` is `refusal`
- THEN the run ends as "didn't finish" rather than reporting success it did not reach

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
