# Agent Desk release readiness

Revised 2026-08-28 after re-verifying the 2026-08-22 audit against the code. The previous
revision of this document listed eight blockers as open; all eight have since been closed
in code. This revision records what is actually true today, and what genuinely remains.

Detailed evidence for the original findings is in `audit-2026-08-22.md`, which is now
historical: read it for the reasoning, not for current status.

## Verdict

**Code-level release blockers are closed. The remaining gate is native acceptance, plus
two live-behaviour questions about the newly added providers and a few product gaps.**

Note the shape of the risk has changed. The old blockers were about GitWyrm's own logic;
what is left is mostly about whether other people's tools behave the way their
documentation says. Those cannot be closed by reading code.

The automated baseline is healthy: TypeScript, the frontend test suite, the Rust test
suite, and the strict OpenSpec validations all pass. The end-to-end safety boundary that
the previous revision said was open is now enforced in two independent layers.

## Previously blocking, now closed

Each row was verified by reading the implementation and its guarding tests.

| Former blocker | How it is closed |
| --- | --- |
| Execution events not safely addressed (repository-keyed link could be overwritten) | `RunSessionLinks.inner` is `HashMap<execution_id, SessionId>`, documented "Never keyed by repository." Events for a superseded execution are dropped as `ExecutionSuperseded`. |
| Read-only was advisory when the provider suppressed the permission request | `cli_agent::denied_tools_for` denies `write` at CLI launch for every read-only intent. Per `copilot help permissions`, a `--deny-tool` outranks every allow rule including `--allow-all-tools`, so it cannot be widened later. The runtime refusal in `airun::cli_run::handle` remains as defense in depth. |
| Review and Summarize created a session but never ran | Kickoff runs them from their source action, with a regression test looping `[Review, Summarize, Fix]`. |
| Kickoff choices did not reach the first run | Provider, mode, and team are carried durably through `agent_kickoff`. |
| Plan had write authority before Start | `WorktreePolicy::NotUntilStart` plus the durable `graph_started_at` field. Durable, so it survives a restart. |
| Graph integration targeted the user's checkout | `ensure_integration_worktree` provisions a dedicated integration worktree; the apply path never targets `session.header.repo_path`. |
| Graph integration could not preserve file semantics | Typed `FileOperation::{Write, Delete, Rename}` and `FileContent::{Text, Binary, Symlink}` carry raw `Vec<u8>` (never UTF-8 decoded), plus a separate executable bit. |
| Orphan reconciliation and losing-start cleanup unwired | `useOrphanResultReconciliation` runs at startup; `cleanup_unused_worktree` handles the losing race. |

## Actually open

### 1. Native acceptance has not been run

This is the real remaining gate and no amount of unit testing substitutes for it. It must
cover: real provider allow rules, dirty checkouts, two simultaneous same-repo sessions,
two real uncommitted helpers, delete/rename/binary changes, conflict and restart,
child-process cleanup, window focus, Split View cross-repo behavior, scaling,
keyboard/screen-reader use, and performance.

No unit fixture that manually commits helper work may substitute for the production
uncommitted-helper scenario.

### 2. ~~Usage and cost are absent from the durable model~~ Closed 2026-08-28

`ExecutionRecord::usage` now accumulates per-turn figures from the provider, and
`agent_session_usage` sums them across every execution (helpers included) as
`providerReported`. The numbers come from the `session/prompt` response, which
`AcpConnection::prompt` previously discarded except for `stopReason`.

Two things remain true about this and are deliberate:

- **Only what the provider reports is recorded.** ACP does not standardise usage.
  Copilot CLI 1.0.80 reports `inputTokens`/`outputTokens`/`cachedReadTokens` but no
  cost; an agent that reports nothing produces no rows at all. GitWyrm never multiplies
  tokens by a price table of its own, so a cost figure appears only when a provider
  states one.
- **`plan_limit` and `plan_reset_at` still have no source.** Provider plan quotas are
  not exposed over ACP, so those two fields stay `None` until something else supplies
  them.

### 3. ~~One transport, one provider~~ Partly closed 2026-08-28

Discovery is now a registry (`ai/agent/registry.rs`) describing four tools: Copilot
(default, unchanged), Gemini CLI, Claude Code via the `@agentclientprotocol/claude-agent-acp`
adapter, and opencode. `Transport` deliberately keeps its single `Cli` variant: all four
are the same transport (a subprocess speaking ACP over stdio) and what differs is data.

**The safety rule, and why it matters more than the count.** ACP standardises no tool
denial at all, so each tool differs: Copilot takes `--deny-tool` (which outranks every
allow rule), the Claude adapter takes `_meta.claudeCode.options.disallowedTools` on
`session/new`, Gemini has only the whole-session `--approval-mode=plan`, and **opencode has
no mechanism whatsoever**. `select::choose` refuses any tool that cannot enforce read-only
for a read-only intent or a Plan before Start, so opencode cannot run Ask, Explain, Review
or Summarize. The check reads the same `policy::check_tool_capability` the engine's own
tool gate uses, so the two cannot drift, and it runs again at `connect`.

Still open here:

- ~~**No picker.**~~ Closed 2026-08-28. `ProviderControl` sits beside the mode and team
  controls, backed by `agent_providers_list`. It shows every known tool with its real
  install state, and disables the ones that cannot run the current chat with the reason
  attached rather than hiding them. Leaving it on "Default AI" stores no choice at all, so
  a chat nobody had an opinion about keeps following the default.
- **`Denial::ReadOnlyMode` is weaker than the other two and is currently treated as equal.**
  Gemini's plan mode is genuinely enforced by its own policy engine, but its
  `exit_plan_mode` tool is auto-allowed when running non-interactively, which is how
  GitWyrm drives it. A Gemini read-only session could in principle leave plan mode without
  ever raising a permission request. Either downgrade Gemini to `Denial::None` for
  read-only work or watch for `switch_mode`.
- **The Claude adapter's `_meta` key is unverified against a live adapter.** A wrong key
  fails *open* (the adapter ignores unknown `_meta` and denies nothing), so this needs one
  empirical check: attempt an `Edit` in a read-only Claude session and confirm refusal.

### 4. `agentDeskPlan.ts` renders opportunistically, and nothing asks for what it reads

Re-examined 2026-08-28. The earlier note called this "a format nothing produces", which
overstates it: `parsePlanChecklist` reads ordinary CommonMark task lists (`- [x] Step`),
optionally with a trailing `(Owner - status)`. Models write that shape unprompted often
enough that the checklist does sometimes render.

What is true is that nothing *guarantees* it. No prompt asks for the convention -- the
structured plan proposal travels as JSON in a fenced block
(`plan_proposal::plan_mode_instruction`), and `RunStep::Plan` is free-form prose. So the
same plan renders as a tidy checklist or as a paragraph depending on how the model felt.

That is a coherent thing to be (progressive enhancement of a common Markdown shape), but
it is not currently a decision anyone made. Either ask for the convention in the Plan
prompt so it is reliable, or drive the checklist from the JSON proposal that already
exists. Leaving it undecided means the feature works by luck.

Not urgent, and explicitly not "delete it": it fails safe, and it costs nothing when a
model writes prose instead.

### 5. Snip is backend-only

`snip_detect` and `snip_gain` exist and are in the bindings, but nothing in the UI calls
them, so there is no user-walkable path yet.

**Corrected 2026-08-28.** An earlier revision of this note said Snip could not reduce Agent
Desk's token use at all, because the agent's shell calls happen inside the provider CLI.
That was wrong about the mechanism. Snip integrates by rewriting the command *inside* the
agent process, through config the agent loads itself: a `preToolUse` hook returning
`modifiedArgs` for Copilot, and a third-party plugin (`opencode-snip`) hooking
`tool.execute.before` for opencode. A host app installs nothing and wraps nothing -- its
only lever is keeping `snip` on the spawned process's PATH.

Three things still qualify that:

- Whether Copilot's hooks fire under `--acp --stdio` is **undocumented**. "ACP" appears
  nowhere in GitHub's hooks reference, and there is precedent for hooks being inert in a
  non-interactive runtime (for the cloud agent, tool calls are pre-approved so the hook
  "does not fire or has no effect"). This needs a real test, not more reading.
- GitWyrm denies `shell` at launch for its own runs, so on the default path there are no
  shell calls to compress regardless.
- Snip passes pipes, redirects, heredocs and command substitution through unfiltered, so
  real savings land well below its headline numbers.

The lever that certainly applies to Agent Desk is the usage tracking in item 2: measuring
cost is what makes any reduction provable.

Two facts in `snip/gain.rs` are inferred from Go's naming defaults rather than a published
schema, and want one capture from a real `snip gain --json` to confirm: the `TotalTimeMs`
spelling (an alias for `TotalTimeMS` is already in place) and the lite-build marker text.

`snip init --agent` is deliberately not implemented: it merges into other tools' config
files and its uninstall matches a substring that would strip unrelated hooks.

## Release order

1. Native acceptance run against a real provider.
2. Verify the two live-behaviour unknowns: the Claude adapter's `_meta` denial actually
   refusing an `Edit`, and Gemini's plan mode holding across a non-interactive
   `exit_plan_mode`.
3. Resolve or remove the plan-checklist parser.

Usage in the durable model is done.

## Meaning of "ready"

Agent Desk is ready when a user can click Fix/Review/Summarize, see the requested run start
immediately, and trust that authority and session identity cannot leak. A graph is ready
when helpers work in isolation, their actual changes combine without touching the user's
checkout, the lead reviews one repository-true result, and landing remains an explicit user
choice. The code now claims all of this; native acceptance is what converts the claim into
evidence.
