# Change: GitWyrm's own agent, driven through provider CLIs

## Why

Running a task in-app needs an agent loop: read files, edit them, run the project's
checks, and stop when the task is done. GitWyrm's AI layer today is a single-shot
`chat()` that returns a string - enough for a commit message, nothing like enough for
this.

The agent has to be **ours and self-contained**. Shelling out to another agent app
(opencode, or any editor's agent mode) would make a core GitWyrm feature depend on
software the user may not have installed, and would put the guardrails - never push,
gate side effects, stop instantly - inside a process we do not control.

What we do reuse is the model access people already pay for, through the provider's own
CLI: Claude Code for Anthropic, the GitHub Copilot CLI for Copilot. The user has already
signed in to those; GitWyrm drives them rather than asking for another API key.

A design spike ("Claude Code CLI Provider for GitWyrm") has been run and **succeeded**:
a proof of concept generated a properly-formatted commit message from a real staged diff
through the local `claude` CLI. Its findings are folded into `design.md` here and shape
several requirements below - the auth check, the absence of any cost UI, and the fact
that a turn takes 10 to 20 seconds rather than the ~1.7s startup floor first assumed.

The spike also stands as the pattern for **any CLI-authenticated tool**, not just Claude:
discover it, ask it whether it is usable, drive it over stdin/stdout, and gate it behind
a version check rather than a pinned path.

Where a provider offers a documented API and the user has a key, GitWyrm should prefer it
- mirroring how opencode talks to providers directly. The CLI path exists so a
subscription-only user is not shut out, not because it is the better transport.

## What Changes

- A `RunDriver` implementation (the trait ships in `add-ai-task-runs`) backed by our own
  agent loop: plan, act, observe, repeat until the task's done-means checks pass
- A tool set the loop can call, and nothing beyond it: read a file, edit a file, list a
  directory, run one of the project's own checks. Every tool is repo-scoped
- Provider-CLI adapters: Claude Code and the Copilot CLI, each detected the way git and
  gpg already are (configured path, PATH, then nothing) with a plain-language message
  when the CLI is missing
- Guardrail enforcement in our process, not the CLI's: linked branch only, push refused
  outright, side effects raised as typed gates the console answers
- The user's uncommitted work set aside before a run and restored after, so "your own
  work is untouched" is literally true and Stop is always safe

## Impact

- Affected specs: `ai-runs` (adds the engine requirements)
- Affected code: new agent module under `src-tauri/src/ai/`, provider-CLI adapters,
  tool implementations, and the driver that wires them to the console's event stream
- Depends on: `add-ai-task-runs` (the trait and the console it feeds),
  `add-ai-provider-surface` (which provider is configured),
  `add-spec-commit-links` (the linked branch a run is allowed to touch)
