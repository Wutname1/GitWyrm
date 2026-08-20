# Change: GitWyrm's own agent, driven through provider CLIs

> **2026-08-19 correction:** what shipped is narrower than what this proposal describes.
> GitWyrm does not run its own plan/act/observe loop - the Copilot CLI runs its own loop
> over ACP and GitWyrm hands it the whole task. Only the Copilot CLI transport was built;
> the BYO API key and OpenAI-compatible transports described under "What Changes" below
> were never implemented. See `tasks.md` for the corrected task list and `design.md` for
> the corrected design notes. The ToS reasoning ("Why" below) is unchanged and is still
> why Anthropic has no CLI path.

## Why

Running a task in-app needs an agent loop: read files, edit them, run the project's
checks, and stop when the task is done. GitWyrm's AI layer today is a single-shot
`chat()` that returns a string - enough for a commit message, nothing like enough for
this.

The agent has to be **ours and self-contained**. Shelling out to another agent app
(opencode, or any editor's agent mode) would make a core GitWyrm feature depend on
software the user may not have installed, and would put the guardrails - never push,
gate side effects, stop instantly - inside a process we do not control.

**The terms research reshapes which transports are allowed, not whether the engine ships.**
Two patterns have to be kept apart, because they carry very different risk (sourced in
`design.md`):

- **Driving the user's own installed CLI as a subprocess** - the CLI authenticates itself,
  GitWyrm never touches a token. No enforcement against this has been found anywhere; it is
  what every healthy multi-agent tool does, and GitHub ships its own Copilot CLI for it.
- **Reading another tool's stored credentials and calling the API directly** - this is what
  drew Anthropic legal action against opencode. **GitWyrm will not do this, for any
  provider.**

So the engine supports several transports from the start, all behind one interface:

1. **Provider CLI as a subprocess** - the Copilot CLI first, since GitWyrm already bundles
   and drives it for commit messages and GitHub built it to be driven.
2. **BYO API key** against a provider's documented API - unambiguously permitted, and the
   fallback whenever a CLI is absent.
3. **An OpenAI-compatible endpoint**, which also covers a local opencode server, Ollama,
   LM Studio, and anything else speaking that shape.

**Anthropic is the exception and is API-key only.** Its terms prohibit routing requests
through subscription credentials on a user's behalf, and it is the one provider that has
actually enforced. No Anthropic subprocess path until that is either clarified or
approved in writing.

Whichever transport is used, the engine uses **the provider the user set as default in AI
settings** - the same one commit-message generation uses. It does not carry its own
provider choice.

A design spike ("Claude Code CLI Provider for GitWyrm") was run and succeeded technically:
a proof of concept generated a properly-formatted commit message from a real staged diff
through the local `claude` CLI. Its engineering findings still hold and are folded into
`design.md` - the auth lesson (ask the tool, never inspect its credential files), the
absence of any cost or token UI, the version-gate approach, and the measured 10-20 second
turn that makes visible progress and streaming mandatory rather than optional.

Its commercial conclusion does not hold. The same spike left terms of service as its one
open question; that question has now been researched and the answer forecloses the
subscription path.

## What Changes (as proposed; see correction note above for what shipped)

- A `RunDriver` implementation (the trait ships in `add-ai-task-runs`) backed by our own
  agent loop: plan, act, observe, repeat until the task's done-means checks pass. **Not
  built** - the CLI runs its own loop; GitWyrm drives the transport only
  (`airun::cli_run::run_task`)
- A tool set the loop can call, and nothing beyond it: read a file, edit a file, list a
  directory, run one of the project's own checks. Every tool is repo-scoped. **Not
  built** as a GitWyrm-side allow-list. What ships is a deny-list at the CLI's server
  start (`shell`, `url` denied; everything else, including the CLI's own broader tool
  set, is left to it) - see `cli_agent::DENIED_TOOLS`
- Three transports behind one `ProviderAgent` interface: a provider CLI driven as a
  subprocess (Copilot CLI first), a documented API with the user's own key, and any
  OpenAI-compatible endpoint (which covers a local opencode server, Ollama, LM Studio).
  **Only the first shipped.** The interface exists; only the Copilot CLI implementation
  is wired to a run
- The engine resolves its provider from the user's **default** in AI settings, never its
  own separate choice - so the Desk and commit messages can never disagree about which AI
  is in use. **Not built this way** - `run_engine` calls `CliAgent::discover` directly and
  does not branch on the configured default provider. There is currently one provider a
  run can use
- No credential-file reading, for any provider, and no Anthropic subprocess path. **Holds**
- Honest handling when the default provider cannot run: say which transport is missing and
  what to do, and leave the copy-handoff path as a full-strength alternative rather than a
  consolation. **Holds** for the Copilot CLI case (not installed / too old / signed out)
- Guardrail enforcement in our process: linked branch only, push refused outright, side
  effects raised as typed gates the console answers. **Partially holds.** Push is refused
  by denying the `shell`/`url` tool kinds the CLI would need to push, and side effects the
  CLI raises come back to GitWyrm as a permission request it answers. Linked-branch
  enforcement (`branch_is_runnable` or equivalent) does not exist in this package; branch
  and worktree isolation are a kickoff-layer concern
- The user's uncommitted work set aside before a run and restored after, so "your own
  work is untouched" is literally true and Stop is always safe. **Not built** - no
  stash-based set-aside exists in `ai/agent/` or `airun/`; isolation is expected to come
  from per-run worktrees at the kickoff layer instead

## Impact

- Affected specs: `ai-runs` (adds the engine requirements)
- Affected code: new agent module under `src-tauri/src/ai/`, provider-CLI adapters,
  tool implementations, and the driver that wires them to the console's event stream
- Depends on: `add-ai-task-runs` (the trait and the console it feeds),
  `add-ai-provider-surface` (which provider is configured),
  `add-spec-commit-links` (the linked branch a run is allowed to touch)
