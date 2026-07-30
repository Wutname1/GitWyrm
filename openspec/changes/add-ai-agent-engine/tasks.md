# Tasks

## 1. Design

- [x] 1.1 Fold the "Claude Code CLI Provider for GitWyrm" spike into design.md: measured
      timings, the auth lesson, the numbers not to trust, and the version-gate finding
- [x] 1.4 Answer the terms-of-service question per provider. Done for Anthropic
      (prohibited in writing) and OpenAI (four unanswered asks; API keys recommended).
      Recorded in design.md, and the reason this change is API-key only
- [x] 1.2 Confirm the Copilot position from GitHub's own terms. Done: Section J of the ToS
      is silent on clients and programmatic access, the AUP's scraping and resale clauses do
      not reach this, and GitHub's own docs say the CLI may be used "as an agent in any
      third-party tools, IDEs, or automation systems". Officially supported - see design.md
- [x] 1.3 Decide the turn budget and what "the task is done" means for the loop, so a run
      cannot spin forever. Done: 12 turns by default, adjustable in settings, counted in
      turns rather than minutes so a task behaves the same on a slow provider as a fast
      one. Done means the targeted task's checkbox is ticked in `tasks.md` - the thing the
      user already sees - with the console's keep-or-undo choice as the real gate. Recorded
      in design.md under "What bounds a run"

## 2. Provider transports

- [x] 2.1 `ProviderAgent` interface with three implementations behind it, chosen by what
      the default provider supports - never by the engine having its own preference.
      Trait and shared types done in `ai/agent/transport.rs`; the three implementations
      are 2.2-2.4 below
- [x] 2.2 CLI subprocess transport: Copilot CLI first. Discovery by PATH then known
      locations, gated on a `--version` floor rather than a pinned path, since these tools
      self-update. Auth state from the CLI's own answer, never its credential files.
      Drives `copilot --acp --stdio` (Agent Client Protocol over NDJSON); `check()` opens a
      real session, since that is the only thing that proves a sign-in has the scope it
      needs. Version floor is a placeholder pending a real install to measure against
- [x] 2.3 API-key transport against a documented API (OpenAI, Anthropic). Multi-turn with
      tool calls in both dialects, which the existing single-shot commit-message client
      has no notion of
- [x] 2.4 OpenAI-compatible endpoint transport, which also serves a local opencode server,
      Ollama, and LM Studio. Same code path as 2.3 with the base URL pointed elsewhere;
      the distinct transport value exists so failures can say "the endpoint you
      configured" rather than naming a provider the user never chose
- [x] 2.5 Anthropic is API-key only - no subprocess path, per design.md. If a user's default
      is Anthropic with no key, say that plainly rather than reaching for the CLI.
      Enforced in `select::choose`, with a test that Anthropic stays on the API path even
      when a CLI is installed
- [x] 2.6 A default provider that cannot run reports which transport is missing and what to
      do, without implying GitWyrm is broken. "Signed in to Copilot but no CLI" and "not
      signed in" are separate sentences because the fix differs; a test asserts no
      explanation reads as a GitWyrm fault
- [x] 2.7 Cancellation terminates any in-flight request or child process promptly, leaving
      no orphan. `kill_on_drop` for an abandoned connection plus an explicit kill when a
      shutdown hangs; proven with a real child process rather than by reading the builder
      call, and the detector itself checked against a live process so the test cannot pass
      vacuously

## 3. The loop

- [x] 3.1 Plan / act / observe loop that ends when the task's done-means checks pass, the
      turn budget is spent, or the user stops. A fourth ending was added deliberately: a
      turn with no tool calls and the task not ticked means the AI gave up, which is
      reported rather than left to run the budget down quietly
- [x] 3.2 Tools, and only these: read file, edit file, list directory, run a project check.
      Every path resolved inside the repository and refused outside it. Lexical check
      first (catches traversal on paths that do not exist yet), then a canonicalize
      re-check for existing targets, which is the only thing that catches a symlink
      pointing out of the repo. `.git` refused case-insensitively. NOTE: the symlink
      case cannot be exercised on an unelevated Windows machine and prints a notice
      instead of asserting - needs verifying where symlinks can be created
- [x] 3.3 Emit the console's typed events as the loop progresses, each with its
      one-sentence plain-language summary. `events.rs` carries the sentence on the event
      so every surface reads identically; a test asserts no refusal or gate uses jargon
- [x] 3.4 Stream a turn's output where the transport allows it, so the console has
      something to show inside the first second or two of a 10-20 second turn. The ACP
      transport surfaces `agent_message_chunk` as it arrives; the API transports return a
      whole turn, which is the shape those endpoints give without SSE

## 4. Guardrails (enforced here, not by the provider)

- [x] 4.1 Refuse any push outright - never a gate, never an option. Checked before any
      other rule, on both the tool name and its arguments, so a push dressed as an
      ordinary edit is still refused
- [x] 4.2 Raise typed gates for side effects: add or remove a dependency, run an install,
      network access, delete files, anything outside the repository. Gate variants defined
      and wired; an unknown tool gates rather than executing. Detecting a dependency or
      install specifically needs the CLI's own tool vocabulary, so those variants are not
      yet raised from a real run
- [x] 4.3 Work only on the linked branch (or a new work branch when none is linked);
      refuse to run with a different branch checked out. `guardrails::branch_is_runnable`;
      the preflight that calls it is part of the run console change
- [x] 4.4 Set the user's uncommitted work aside before the run and restore it after,
      reusing the stash plumbing. One honest narrowing: a *kept* run leaves the stash in
      place and says so, because restoring over the run's own edits would conflict. A
      visible stash is recoverable; a failed automatic merge is a mess the user did not
      ask for
- [x] 4.5 Cancel promptly on stop, including mid-turn, leaving the tree in a state the
      console's keep/undo choices can act on. The stop flag is checked before each turn
      and before each tool, so a stop lands within a step rather than at the end of a run

## 5. Verify

Section 5 needs credentials, installed CLIs, and the run console (which is
`add-ai-task-runs`, not this change). What could be verified here was; the rest is
listed as needing a human, with the reason.

- [~] 5.1 A real task run end to end against Claude Code on a scratch repository.
      **Superseded by 2.5.** Claude Code is installed on the dev machine (2.1.220), but
      this change deliberately has no Anthropic subprocess path -- their terms prohibit
      routing requests through subscription credentials, and that is the one provider
      where enforcement has been observed. Running this task as written would build the
      thing 2.5 exists to prevent. The real version string is kept as a parser fixture
- [~] 5.2 The same task against the Copilot CLI. **Partly done.** Copilot CLI 1.0.76 is
      now installed, and the transport was verified against it: the `initialize`
      handshake succeeds (protocolVersion 1 matches, agentInfo.name is "Copilot"), and
      the `--deny-tool=shell --deny-tool=url` flags are accepted. The version floor is
      now measured (1.0.0, set no higher than what was confirmed working). A full task
      run still needs a signed-in Copilot account - device-flow login is interactive
- [ ] 5.3 The same task against a direct provider API with a key, proving the interface
      is not CLI-shaped. **Needs a key.** The dialect shapes are unit-tested against
      recorded request/response bodies, which is not the same as a live call
- [x] 5.4 Guardrails hold under a hostile prompt: ask it to push, to install a package,
      and to edit a file outside the repo - push refused, the others gated. Verified as
      unit tests driving the loop with a scripted provider: a push never reaches the
      tool, an unknown tool gates, and a path outside the repo gates. A live hostile
      prompt is still worth running once a provider is wired
- [x] 5.5 Stop mid-turn leaves no half-written file, no orphaned process, and the user's
      own work intact. Orphan case proven against a real child process, with the
      detector itself checked against a live process so it cannot pass vacuously. The
      "no half-written file" half is structural - edits are whole-file writes - but is
      worth confirming in a real run
- [ ] 5.6 With no provider CLI installed and no key, the Desk shows the reconnect state
      and the copy-handoff path still works. **Needs the run console UI**
- [ ] 5.7 An account with credentials but a bad scope reports needs-reconnect rather than
      failing at generation time (the spike's own failure case). **Needs such an
      account.** The code path exists: `check()` opens a real session rather than looking
      for a credential file, and 401/403 maps to needs-reconnect
