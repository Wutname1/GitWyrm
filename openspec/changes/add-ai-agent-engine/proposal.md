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

A design spike ("Claude Code CLI Provider for GitWyrm") covers the Claude side and is
the reference for the adapter shape here; its findings should land as this change's
design.md before build tasks start.

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
