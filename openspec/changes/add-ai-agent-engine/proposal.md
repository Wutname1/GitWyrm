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

**The subscription-reuse plan does not survive the terms research.** The original intent
was to reuse the model access people already pay for by driving Claude Code and the Codex
CLI against their existing sign-ins. Anthropic prohibits that in writing and enforces it;
OpenAI has declined four times to say it is allowed. Both are documented in `design.md`.

So the engine is built on **BYO API key**, which is unambiguously permitted and is what
both providers' own docs recommend for programmatic use. That is a smaller feature than
planned, and worth being plain about: a user with only a Claude subscription and no API
key cannot run tasks in GitWyrm. The honest response is to say so in the UI and keep the
copy-handoff path first-class, not to route their subscription credentials anyway.

A design spike ("Claude Code CLI Provider for GitWyrm") was run and succeeded technically:
a proof of concept generated a properly-formatted commit message from a real staged diff
through the local `claude` CLI. Its engineering findings still hold and are folded into
`design.md` - the auth lesson (ask the tool, never inspect its credential files), the
absence of any cost or token UI, the version-gate approach, and the measured 10-20 second
turn that makes visible progress and streaming mandatory rather than optional.

Its commercial conclusion does not hold. The same spike left terms of service as its one
open question; that question has now been researched and the answer forecloses the
subscription path.

## What Changes

- A `RunDriver` implementation (the trait ships in `add-ai-task-runs`) backed by our own
  agent loop: plan, act, observe, repeat until the task's done-means checks pass
- A tool set the loop can call, and nothing beyond it: read a file, edit a file, list a
  directory, run one of the project's own checks. Every tool is repo-scoped
- Provider access by **API key only**, through each provider's documented API - the path
  both Anthropic and OpenAI affirmatively recommend for programmatic use
- Honest handling of the subscription-only user: the Desk says a key is needed, links to
  where to get one, and leaves the copy-handoff path as a full-strength alternative rather
  than a consolation
- Guardrail enforcement in our process: linked branch only, push refused outright, side
  effects raised as typed gates the console answers
- The user's uncommitted work set aside before a run and restored after, so "your own
  work is untouched" is literally true and Stop is always safe

## Impact

- Affected specs: `ai-runs` (adds the engine requirements)
- Affected code: new agent module under `src-tauri/src/ai/`, provider-CLI adapters,
  tool implementations, and the driver that wires them to the console's event stream
- Depends on: `add-ai-task-runs` (the trait and the console it feeds),
  `add-ai-provider-surface` (which provider is configured),
  `add-spec-commit-links` (the linked branch a run is allowed to touch)
