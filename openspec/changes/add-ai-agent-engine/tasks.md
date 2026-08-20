# Tasks

> **2026-08-19 correction:** this package originally described an in-house
> plan/act/observe loop reached through three transports (provider CLI, BYO API
> key, OpenAI-compatible endpoint). The codebase was refactored since: GitWyrm
> does not run the loop any more. The provider CLI plans, acts, and observes on
> its own; GitWyrm hands it the whole task and drives only the transport (see
> `src-tauri/src/ai/agent/mod.rs`, `src-tauri/src/airun/cli_run.rs`). Only the
> Copilot CLI transport is wired in production, via ACP
> (`src-tauri/src/ai/agent/acp.rs`, `cli_agent.rs`, `copilot_cli.rs`). The
> `Transport::ApiKey`/`OpenAiCompatible` variants and `select::choose` describe
> transports that were never built and are removed as dead code (see below).
> Sections 1-4 are rewritten to match what ships. Section 5 keeps its
> verification history since that's a record of what was actually checked, with
> 5.3 corrected to no longer describe a path that doesn't exist.

## 1. Design

- [x] 1.1 Fold the "Claude Code CLI Provider for GitWyrm" spike into design.md: measured
      timings, the auth lesson, the numbers not to trust, and the version-gate finding
- [x] 1.4 Answer the terms-of-service question per provider. Done for Anthropic
      (prohibited in writing) and OpenAI (four unanswered asks; API keys recommended).
      Recorded in design.md, and the reason Anthropic has no CLI path
- [x] 1.2 Confirm the Copilot position from GitHub's own terms. Done: Section J of the ToS
      is silent on clients and programmatic access, the AUP's scraping and resale clauses do
      not reach this, and GitHub's own docs say the CLI may be used "as an agent in any
      third-party tools, IDEs, or automation systems". Officially supported - see design.md
- [x] 1.3 Decide the turn budget and what "the task is done" means for the loop, so a run
      cannot spin forever. Done: 12 turns by default, adjustable in settings, counted in
      turns rather than minutes so a task behaves the same on a slow provider as a fast
      one. Done means the targeted task's checkbox is ticked in `tasks.md` - the thing the
      user already sees - with the console's keep-or-undo choice as the real gate. Recorded
      in design.md under "What bounds a run". **Superseded in practice**: the CLI runs its
      own loop, so "turn budget" is no longer something GitWyrm counts - see 3.1 below

## 2. Provider transport (CLI only)

The three-transport design (CLI subprocess, BYO API key, OpenAI-compatible endpoint) was
the original plan. Only the CLI subprocess transport was ever built, and it is the only
one that ships. There is no in-house loop turning single-provider API calls into a run;
`ai::agent::run::SYSTEM_PROMPT` is a system prompt handed to the CLI's own agent, not a
prompt/act/observe driver GitWyrm implements.

- [x] 2.1 `ProviderAgent` shape narrowed to the one thing GitWyrm actually drives: a
      provider CLI's ACP server. Shared transport/error vocabulary lives in
      `ai/agent/transport.rs` (`Transport`, `AgentError`); the connection itself is
      `ai/agent/acp.rs` (`AcpConnection`)
- [x] 2.2 CLI subprocess transport: Copilot CLI, and only the Copilot CLI. Discovery by
      PATH then known locations (`copilot_cli.rs`), gated on a `--version` floor rather
      than a pinned path, since these tools self-update. Auth state from the CLI's own
      answer, never its credential files. Drives `copilot --acp --stdio` (Agent Client
      Protocol over NDJSON); connecting opens a real session, since that is the only thing
      that proves a sign-in has the scope it needs. Version floor: 1.0.0, measured against
      a real install (1.0.76)
- [x] 2.3 ~~API-key transport against a documented API (OpenAI, Anthropic).~~ **Not
      built.** No dialect/driver exists for calling a provider's HTTP API directly with a
      multi-turn tool-using loop. `ai::client` remains the single-shot `chat()` used for
      commit messages; it is not a task-run transport
- [x] 2.4 ~~OpenAI-compatible endpoint transport.~~ **Not built**, for the same reason as
      2.3: there is no code path that speaks the OpenAI dialect for task runs
- [x] 2.5 Anthropic has no CLI path - this still holds and needs no code, since the CLI
      transport is Copilot-only in the first place. `run_engine` in
      `commands/airun.rs` calls `CliAgent::discover` directly; it never routes by provider
      ID, so there is no branch that could reach for a Claude subprocess
- [x] 2.6 A default provider that cannot run reports which piece is missing and what to
      do, without implying GitWyrm is broken. `CliAgent::discover` distinguishes "not
      installed" from "too old" from (via `connect()`) "installed but not signed in
      correctly"; `select::plain_explanation` turns each into a sentence that never blames
      GitWyrm (tested in `select.rs`)
- [x] 2.7 Cancellation terminates any in-flight request or child process promptly, leaving
      no orphan. Proven with a real child process rather than by reading the builder
      call, and the detector itself checked against a live process so the test cannot pass
      vacuously

## 3. The run

There is no plan/act/observe loop implemented in GitWyrm for this transport - the Copilot
CLI runs its own agent loop over ACP and reports as it goes. `ai::agent::run` module docs
say this plainly: "GitWyrm does not run the loop itself."

- [x] 3.1 Hand the whole task to the CLI's own agent loop in one `session/prompt`, and
      drive it to a `stopReason` (`end_turn`, `max_turn_requests`, `refusal`, `cancelled`)
      rather than feeding it single turns. `airun::cli_run::run_task` is the driver;
      `CliAgent::turn` refuses to be called for single-turn use so a caller cannot
      silently do half a run. There is no GitWyrm-side turn budget for this transport -
      the CLI's own `max_turn_requests` stop reason is what "ran out of turns" now means
- [x] 3.2 ~~Tools, and only these: read file, edit file, list directory, run a project
      check.~~ **Not what ships.** The Copilot CLI keeps its own broader tool set;
      GitWyrm denies only `shell` and `url` at server start (`cli_agent::DENIED_TOOLS`,
      verified against `copilot help permissions` on 1.0.76). `write` is deliberately not
      denied - editing files in the repository is the job. There is no GitWyrm-side
      allow-list of file operations, and no separate lexical/canonicalize path check in
      this module - path scoping is Copilot CLI's own concern once shell and network
      access are denied
- [x] 3.3 Emit the console's typed events as the run progresses, each with its
      one-sentence plain-language summary. `cli_run::handle` turns `Incoming::TextChunk`,
      `Incoming::ToolCall`, and `Incoming::PermissionRequest` into `RunStep`s for the
      console; `RunStep`'s summaries live in `airun::driver`
- [x] 3.4 Stream a turn's output as the ACP transport allows it, so the console has
      something to show inside the first second or two. `session/update` notifications
      with `agent_message_chunk` arrive as `Incoming::TextChunk` and are forwarded as they
      come in

## 4. Guardrails GitWyrm still enforces

The CLI plans and acts on its own, so guardrails here mean what GitWyrm's process denies
or refuses to hand the CLI, not a separate in-house enforcement layer sitting between the
model and a bounded tool set.

- [x] 4.1 Push is not a capability the CLI is given: `shell` and `url` are denied tool
      kinds at server start (`DENIED_TOOLS`), and denial takes precedence over every allow
      rule the CLI has, including `--allow-all-tools`. `ai::agent::run::SYSTEM_PROMPT` also
      tells the model plainly it can never push, but the enforced boundary is the denied
      tool, not the prompt
- [x] 4.2 Side effects beyond in-repo file edits are unreachable rather than gated one by
      one: `shell` and `url` cover running installs, reaching the network, and anything
      else that isn't a file read/write inside the working directory. A permission request
      the CLI raises for anything else (`session/request_permission`) comes back to
      GitWyrm and is answered `allow_once`/`reject_once`/`cancelled` -
      `cli_run::handle`/`pick` - and GitWyrm SHALL never answer `allow_always`
- [ ] 4.3 ~~Work only on the linked branch; refuse to run with a different branch checked
      out.~~ **Not implemented in this package.** No `branch_is_runnable` or equivalent
      preflight exists in the current code. Branch/worktree isolation for a run is a
      concern of the run-kickoff and worktree-provisioning work
      (`agent-desk-source-kickoffs`, `git/worktree.rs`), not this transport package
- [ ] 4.4 ~~Set the user's uncommitted work aside before the run and restore it after,
      reusing the stash plumbing.~~ **Not implemented in this package.** No stash-based
      set-aside/restore exists in `ai/agent/` or `airun/`. Whether a run needs an isolated
      worktree (which sidesteps needing a stash at all) is decided at the kickoff layer;
      see `architecture.md` section 8 ("For Fix, provision an isolated worktree before the
      first edit")
- [x] 4.5 Cancel promptly on stop, including mid-turn, leaving the tree in a state the
      console's keep/undo choices can act on. `session/cancel` is a notification the ACP
      spec requires the agent to answer with `stopReason: "cancelled"`, so Stop has a
      defined completion rather than a dropped pipe. Proven with a real child process

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
- [x] 5.2 The same task against the Copilot CLI. Copilot CLI 1.0.76 installed and signed
      in (free tier). `tests/copilot_acp.rs` drives GitWyrm's own ACP client against it
      and opens a real session with shell and network access denied. The test skips
      itself when the CLI is absent or signed out, so it is safe elsewhere. Version floor
      measured at 1.0.0
- [x] 5.3 ~~The same task against a direct provider API with a key, proving the interface
      is not CLI-shaped.~~ **Rewritten: there is no direct-API path to verify.** The
      three-transport design this task assumed was never built past the Copilot CLI - see
      section 2 above. `Transport::ApiKey` and `select::choose`, the only code that ever
      implied an API-key run path existed, are dead (test-only) and have been removed.
      Closing this task as no-longer-applicable rather than leaving it looking like
      pending work
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
      account.** The code path exists: `connect()` opens a real session rather than looking
      for a credential file, and a failed session start maps to `AgentError` variants the
      console can show
