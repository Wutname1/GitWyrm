# Agent Desk release readiness

Recorded 2026-08-21, after reconciling all nine `openspec/changes/agent-desk-*/tasks.md`
task lists against the code actually on this branch. This is a reconciliation, not a new
audit from scratch — see each package's own "Status 2026-08-21" section for the file-level
evidence. This document synthesizes across packages: what a user can do end to end today,
what looks present but is not usable, the native checks a human still has to run, and an
honest verdict.

## What a user can actually do end to end today

- Open Agent Desk from a repository (one stable app-wide window, not a new one per repo).
- Click Fix/Plan/Explain on an issue or Review/Summarize on a pull request. A session is
  created, the window focuses and selects it via a real event
  (`agent-desk://select-session`), and for write-capable intents (Fix) the engine starts
  automatically — no second message required.
- Type a message, pick Ask/Plan/Auto and Solo/Lead+helpers, and send. The chosen mode is
  genuinely enforced: `ExecutionPolicy::resolve` runs before any worktree/CLI/write side
  effect, and a live model that tries to write under a read-only intent is refused before a
  disk write or an approval gate, proven against the real production `handle()` function.
- Watch the run stream into the transcript, answer approval gates, and stop it (one
  execution or all) through a real process-cancellation registry.
- In Plan mode, get a real proposed graph back (parsed from the model's own fenced reply),
  see it as an AwaitingStart card, and click Start. Up to three helpers genuinely launch as
  separate CLI processes in their own worktrees, with their own turn/wall-clock budgets,
  their own approval-gate channel, and their own Stop button.
- When a run finishes, a Result Review panel appears automatically in that same
  conversation, showing changed files and check outcomes, with a "View diff" button that
  opens the real diff viewer in the main window against that worktree.
- Keep or Undo a solo/lead result, or draft a commit message and commit; draft a pull
  request with editable title/body before anything touches the host. None of this ever
  pushes on its own.
- Reach an Import tab from the Agent Desk title bar, scan for Codex/Claude Code/OpenCode
  sessions, import one, and click "Continue here" to hand it a native segment.
- Open Agent Setup, see a skills/MCP-connector inventory across clients, preview an
  individual copy with a per-destination diff, apply it with a receipt and Undo, or run
  "Match selected apps" as a real batch that shows every per-item plan before anything is
  written.

This is substantially more than existed at the start of this session. The R1 (execution
authority), R3 (source-bound solo loop), and large parts of R6/R7 (real helper execution,
reachable import, batch preview) gates described in `implementation-reset-2026-08-21.md`
are now backed by genuine, traced call chains from a production entry point to a visible
result, not just isolated components or passing unit tests.

## What looks present but is not usable

- **Helper results are never actually integrated.** This is the most serious gap found this
  session. A helper runs in its own worktree, and when it finishes cleanly, nothing ever
  writes its changes into the lead's tree — no `fs::write`, no git apply, no commit exists
  anywhere in `integrate_helper_result`. When a conflict is detected and the user picks
  "Keep helper" / "Keep integrated" / a merged version, the backend computes the correct
  text but never writes it anywhere either; "resolving" only flips the record to Finished
  and clears the conflict marker. A multi-helper graph can run to completion and the user
  is left with several isolated worktrees and no consolidated result. This is the core
  promise of the graph feature and it does not exist yet.
- **A helper's conflict state does not survive a page/session reload.** The helper's process
  is removed from the live execution registry before the conflict is even recorded on its
  execution. Session reconciliation (which runs on every session load, not just app restart)
  cannot distinguish "waiting on an approval gate" from "waiting on a conflict decision" —
  both are `NeedsInput` — so a conflicted helper is silently reclassified `Interrupted` the
  next time anyone opens that session. A conflict that isn't resolved in the same window
  session it occurred in effectively disappears.
- **OpenSpec task completion has no UI path.** The backend command and frontend hook are
  both correct and call the real shared task-line writer, but no component anywhere imports
  the hook. There is currently no way, from the running app, to tick an OpenSpec task from
  an accepted Agent Desk result.
- **"Request revision" doesn't request anything automatically.** Clicking it flips the
  result to a RevisionRequested state and shows a toast asking the user to type a follow-up
  message themselves — it does not append a message or start a new execution turn, despite
  the task calling for exactly that.
- **A message sent while the agent is Working is not delivered to that run.** It is saved to
  the transcript and surfaced with an honest toast ("saved for the next turn"), which is the
  correct fallback per the reset doc's instruction to never claim false delivery — but there
  is still no real steering channel into a live execution.
- **Match selected apps (batch config sync) auto-selects every eligible destination** for
  every differing item; there is no per-item destination picker in the batch flow the way
  there is for a single item, so a user cannot exclude one destination from an otherwise
  bulk sync.
- **Orphaned worktree/result reconciliation is not wired to app startup.** The detection
  command and its test both exist; nothing calls it when the app launches.
- Several graph-panel affordances are backend-ready but have no frontend button yet: a
  helper's own "View diff" scoped to its node, and the "Open" action for a worktree kept
  because it has hand edits.

## Native checks a human must run

None of `docs/agent-desk/acceptance-checklist.md`'s items are checked, and this audit did
not check any of them either — every one requires a running packaged Tauri app, which no
agent in this environment can launch or observe. The checklist's own items, plus each
package's native-only tasks, group into:

- **Window/session identity**: Spec Desk entry points open one Agent Desk window; repeated
  Open/Fix/Review focus it rather than creating a second one; an already-open Desk selects
  a freshly kicked-off session (only string-matched by a unit test today, never driven
  through a real window).
- **Display and accessibility**: 100%/125%/150%/200% Windows scaling for session rows, the
  three-column shell, and the config-sync tables; keyboard-only paths through session list,
  history rail, and composer; screen-reader names; focus return from popovers; reduced
  motion.
- **Scale and performance**: 1,000 session headers scrolling without frame drops; two live
  transcripts and an active three-helper graph profiled together; 1,000-row config-sync
  tables.
- **Process lifetime**: restart recovery of selection/title/source/transcript/result; a
  crashed app recovering every worktree that holds the only copy of work; Stop verified to
  leave no child process running after the app exits; offline/reconnect/cancel/conflict
  scenarios end to end.
- **Cross-pane/cross-repo correctness**: two repositories with two running sessions in Split
  View; per-pane Source/Context/Graph never leaking another pane's session; main-window repo
  switching without losing a pane's selection.
- **Graph-specific**: two independent helper edits, one stopped mid-run, simultaneous
  approval gates on two helpers, a forced same-line conflict, and a crash — none of the
  automated tests substitute for actually watching this happen in a live client.
- **Config sync**: concurrent-edit refusal and partial-batch-failure behavior against a real
  filesystem outside the fixture harness.

None of these can be honestly checked by reading code, and none were checked in this pass.

## Verdict

**Not releasable yet, but meaningfully closer than the 2026-08-21 reset's starting point.**

The specific behavioral failures the reset doc opened with are substantially resolved:
Stop now cancels a real process; Ask/Plan/Review/Summarize genuinely refuse writes at the
engine boundary, proven against production code; issue Fix auto-starts without a second
message; helpers are provisioned *and launched*, not just recorded; results and review UI
are mounted from live completion; OpenSpec context reaches the live prompt; Import is
reachable; and Match selected apps shows a real per-item preview before writing.

What blocks a release verdict is not native-only items (those were always going to be last)
— it is the **helper-integration gap**: the graph feature's central value, combining
multiple agents' work into one reviewable result, does not happen. A user can run a
three-helper graph today and end up with nothing usable beyond manually opening each
worktree by hand. Shipping the graph feature as currently built would mislead users about
what "the lead reviews and consolidates helper work" means. Either the integration-apply
step needs to be built (even a minimal git-apply-and-commit-per-node), or the graph feature
needs to stay behind whatever gate keeps users from reaching it until that exists.

Secondary but real: the conflict-state-doesn't-survive-a-reload bug means even a user who
stays in the same window to resolve a conflict immediately is racing a background
reconciliation pass that can erase the conflict marker first. This needs a fix (exclude
conflict-marked executions from the live-process reconciliation check, or use a distinct
state) before conflict handling can be trusted at all.

Every other package's remaining gaps are either disclosed, scoped follow-ups (Codex/Copilot/
OpenChamber writers, unlink for imports, per-item batch destination selection) or native
verification that legitimately cannot happen without a human at the keyboard. Those are
normal pre-release punch-list items, not misrepresentations.
