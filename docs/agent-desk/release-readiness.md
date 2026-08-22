# Agent Desk release readiness

Revised 2026-08-22 after the second code audit. This document is the current verdict; the
implementation plan is `implementation-reset-2026-08-21.md` and detailed evidence is in
`audit-2026-08-22.md`.

## Verdict

**Not releasable. Keep Agent Graphs and one-click AI behind their current development gate.**

The automated baseline is healthy: TypeScript, 683 frontend tests, 1,115 Rust tests (one
ignored), and all 17 strict OpenSpec validations pass. Several real paths now exist:
durable sessions, targeted session selection, process cancellation, helper launch,
approval routing, solo result review, import entry, and configuration preview. These are
useful foundations, but they do not close the end-to-end safety boundary.

## Blocking behavior

1. **Execution events are not safely addressed.** A repository-keyed live link can be
   overwritten by a second session for the same repository, allowing cross-chat event and
   result contamination.
2. **Read-only is not a hard provider boundary.** Runtime permission refusal is bypassable
   when provider configuration suppresses the request. The provider must launch with write
   denied for read-only operations and Plan before Start.
3. **Review and Summarize do not run from their source action.** They create a session but
   automatic execution is currently limited to write-capable policy.
4. **Kickoff choices do not reliably reach the first run.** Provider override, mode, and team
   need durable handoff; configured default provider selection must replace hard-coded
   fallback behavior.
5. **Plan has write authority before Start.** The proposal run is invoked as already started.
6. **Graph integration is unsafe and incomplete.** It targets the user's open checkout,
   ignores uncommitted helper work, and cannot preserve deletion, rename, binary, symlink,
   executable-bit, or file-mode semantics.
7. **A finished graph has no combined reviewable truth.** There is no required lead review
   over a dedicated integration worktree and no primary result that represents all helpers.
8. **Important completion paths remain unwired.** Exact OpenSpec task acceptance, real
   revision execution, automatic orphan reconciliation, helper-scoped diff navigation, and
   losing-start cleanup are still open.

## Release order

The release gates are intentionally ordered:

1. execution-addressed routing and hard authority;
2. isolated, operation-faithful graph integration;
3. durable conflict/restart behavior and combined lead review/result;
4. truthful one-click source actions and provider/mode/team propagation;
5. OpenSpec/revision/recovery completion paths;
6. automated regressions, then native Windows acceptance.

Native validation must cover real provider allow rules, dirty checkouts, two simultaneous
same-repo sessions, two real uncommitted helpers, delete/rename/binary changes, conflict and
restart, child-process cleanup, window focus, Split View cross-repo behavior, scaling,
keyboard/screen-reader use, and performance. No unit fixture that manually commits helper
work may substitute for the production uncommitted-helper scenario.

## Meaning of “ready”

Agent Desk is ready only when a user can click Fix/Review/Summarize, see the requested run
start immediately, and trust that authority and session identity cannot leak. A graph is
ready only when helpers work in isolation, their actual changes combine without touching
the user's checkout, the lead reviews one repository-true result, and landing remains an
explicit user choice.
