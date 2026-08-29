# Agent Desk release readiness

Revised 2026-08-28 after re-verifying the 2026-08-22 audit against the code. The previous
revision of this document listed eight blockers as open; all eight have since been closed
in code. This revision records what is actually true today, and what genuinely remains.

Detailed evidence for the original findings is in `audit-2026-08-22.md`, which is now
historical: read it for the reasoning, not for current status.

## Verdict

**Code-level release blockers are closed. The remaining gate is native acceptance, plus
three product gaps that are not safety issues but do affect whether the feature is worth
shipping.**

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

### 2. Usage and cost are absent from the durable model

`agent_session_usage` reports `session_requests` and `active_helper_count` and leaves
tokens and cost as `None`, which is honest but empty. For a feature whose premise is
running budgeted parallel helpers, not recording what a run cost is a correctness gap
rather than a missing convenience. Tracked separately; see the usage work in
`openspec/changes/`.

### 3. One transport, one provider

The engine reaches exactly one runtime: the GitHub Copilot CLI over ACP. `Transport` has a
single `Cli` variant and discovery hardcodes `copilot` binary names. The ACP layer itself
is protocol-generic, so this is a discovery-layer limit rather than a protocol one.
Tracked separately.

### 4. `agentDeskPlan.ts` parses a format nothing produces

`parsePlanChecklist` expects `- [x] Step (Owner - status)` Markdown. Plan proposals travel
as JSON in a fenced block, and `RunStep::Plan` is free-form prose. The parser returns an
empty list and `PlanChecklist` mounts nothing. It fails safely, but it is dead code
pretending to be a feature: either wire it to the real proposal or delete it.

## Release order

1. Native acceptance run against a real provider.
2. Usage in the durable model.
3. Additional ACP transports.
4. Resolve or remove the plan-checklist parser.

## Meaning of "ready"

Agent Desk is ready when a user can click Fix/Review/Summarize, see the requested run start
immediately, and trust that authority and session identity cannot leak. A graph is ready
when helpers work in isolation, their actual changes combine without touching the user's
checkout, the lead reviews one repository-true result, and landing remains an explicit user
choice. The code now claims all of this; native acceptance is what converts the claim into
evidence.
