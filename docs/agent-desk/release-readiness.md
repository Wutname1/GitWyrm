# Agent Desk release readiness

Revised 2026-09-02 after a second outside audit and the build slice that answered it.
The 2026-08-28 revision said "code-level release blockers are closed"; the audit that
followed showed that was true of the original eight blockers and false of the product
as a whole. This revision keeps score honestly: what landed, what is verified, and what
is still a claim.

Detailed evidence for the original findings is in `audit-2026-08-22.md`, which is
historical: read it for the reasoning, not for current status.

## Verdict

**Agent Desk now does the things its screens say it does, and none of it has been
walked end to end in the native app by a person.** That second half is the gate.

What changed since the last revision, each verified by unit tests and, where noted, by a
real run:

| Area | Before | Now |
| --- | --- | --- |
| Conversation | Each turn sent only the newest user message | The whole transcript (user and assistant, imported messages included, tool noise excluded) is handed over every turn, shortened from the oldest end under a 48k-character budget with a visible note |
| Chat identity | Mode, team and AI tool were pane state; new chats were bound to the open repo with no way to change | Saved on the session header; a new chat shows its project (changeable before the first message) and what started it; the sidebar shows the AI tool's logo |
| Auto with helpers | One lead ran while the UI said "up to 3 helpers" | An Auto lead may return a helper plan and it starts immediately through the same launch path as Plan's Start button |
| Auditor | Verdict assigned and never read; a correction turn's result was discarded and could hang on a permission request | Correction turns are driven like any turn, re-audited, and a still-hollow run ends Failed with reasons. Verified live 2026-09-02 (`auditor_catches_a_hollow_codex_run`): Codex was told to ship `is_even` returning True; the auditor found two problems, sent them back, the correction rewrote it as `n % 2 == 0` with the four tests, and the re-audit passed. The same scenario passes on Copilot (`GITWYRM_LIVE_PROVIDER=copilot`) |
| Shell | Denied for every run | Follows the write decision: allowed on Auto, Fix and started Plan through the approval gate; denied for read-only chats. Network stays denied |
| Claude Code | Launched with `--safe-mode` and an empty MCP config; every prompt auto-denied | Loads the user's own MCP servers and skills; prompts reach GitWyrm's gate over stdio; Bash restored on writing runs. Unverified live (login expired) |
| Codex | Read-only chats launched `codex app-server --sandbox=read-only`, which the binary rejects; stderr unread; the chat "did nothing". Approvals were answered `approved`/`denied`, words Codex reads as a refusal, so every Allow still ended in "write access was denied" | Sandbox travels in `thread/start`; stderr tail is kept and shown; unknown server requests are declined instead of hanging the turn; approvals use the schema's own `accept`/`decline`/`cancel`, `item/permissions/requestApproval` is granted for the turn, and file-change gates name their files. Verified: a real read-only turn answered PONG and a real writing turn's edits landed |
| Queued messages | A message sent during a turn was saved and never read | A clean Finish with nothing else live starts the next turn with the same mode/team/tool after a note in the chat. Stopped or Failed turns never restart, and say so rather than leaving the message unexplained |
| Usage | Transcript rows counted as "turns"; helper context could replace the lead's | Turns only when reported; context from the lead; per-agent rows for team runs; unreported figures stay blank |
| Deleting a chat | Failed every time (wrong Tauri state type) | Works |
| Windows | Restored off-screen after monitor changes | Clamped back onto a visible screen on load |

The automated baseline is healthy: TypeScript, the frontend suite (873), the Rust suite
(1474, 10 ignored: real-binary runs), the production frontend build, and strict OpenSpec
validation (25 items) all pass. These numbers go stale on their own; treat the suites as
the source of truth and re-read them before quoting.

## Failure paths repaired 2026-09-02 (second outside audit)

An outside audit read the failure paths rather than the normal ones and found six
places where an unattended run could hang or report success it had not earned. All six
are fixed, with a test each:

| Finding | Was | Now |
| --- | --- | --- |
| A graph reported Finished without a valid result | Finished even when the lead's review failed or was stopped, or the combined result could not be saved; the reason went in a note | `graph_finish_outcome` ends the run Stopped or Failed with the reason; Finished requires a finished review AND a saved result |
| Failed dependency stranded downstream helpers | A helper waiting on a Failed/Stopped/Interrupted helper stayed Draft forever, and completion refused to proceed because it was not terminal | The scheduler reports them as abandoned; each is marked Failed with a note naming what it waited for, so the run ends |
| A zero-helper proposal parked the graph in Working | Start treated "no helpers" as a team, and completion saw nothing to do | Start reads it as the lead working alone: the plan is cleared and the chat handed back Ready |
| Stop did not stop the auditor | The audit ran on its own fresh connection, so Stop reached only the working agent | The cancel handle reaches the audit; a cancelled check is Unavailable, which lets the work through |
| The auditor missed staged changes | `diff_index_to_workdir` hid anything the agent had `git add`ed, so staging was a one-command way past the check | The diff runs from the run's starting commit to the working directory, seeing staged and unstaged alike |
| Deleting an imported chat left a ghost | The results sidecar stayed on disk and the import ledger still pointed at the deleted session | Delete unlinks the import first, then removes the session and its sidecar |

Two of these (the graph's own finalization and the dependency propagation) are only
reachable through a real multi-helper run, so they remain unproven outside their unit
tests until native acceptance covers a real Auto graph.

## Still a claim: what native acceptance must cover

No unit fixture substitutes for these. Each is a path a person walks in the built app.

1. Open a chat on each of Copilot, Claude Code and Codex with nothing extra installed and
   get a reply. **Done 2026-09-12**: all three answer a real turn.
   `codex_answers_a_real_turn` and `claude_answers_a_real_turn` were both run against live
   binaries (the Claude login that blocked this is no longer expired), and Copilot's
   `lists_models_and_answers_a_prompt` completes a turn once its assertion stopped gating
   on the runner's GitHub plan. Still a harness rather than the app, so the chat surface
   itself is unproven -- but "the provider answers" no longer is.
2. Send a second message while a turn is running and watch the follow-up turn start on
   its own when the first finishes, and NOT start after pressing Stop. After Stop, the
   chat should now say the last turn ended before reading the message and that sending
   again starts a new turn -- added 2026-09-16, because refusing to restart was right but
   was being done in silence, under a toast that had promised pickup either way.
3. An Auto chat whose lead proposes helpers: the helpers appear and run without a Start
   button; the same chat in Plan mode waits for Start.
4. A run whose agent runs a command: the approval gate appears, Allow runs it, Reject
   refuses it, and a read-only chat never shows the gate at all.
5. The auditor against a deliberately under-specified task where the agent ticks every
   box and ships a stub. Done once, from a test harness rather than the app: see the
   Auditor row above. Repeat it from the app so the notes and the final state are seen
   where a person would see them. A run that changed nothing now ends "Finished, but
   nothing in the project was changed" instead of claiming changes are ready.
6. The four paths the second audit could only reach in unit tests: a real Auto graph
   that spawns helpers, Stop pressed during the after-run check, a helper whose
   dependency fails, and deleting an imported chat.

   **Re-checked 2026-09-12: all four ARE covered, and the wording above misreads as
   "uncovered" when the gap is narrower than that.** `schedule_starts_independent_ready_helpers_up_to_the_cap`
   and `a_helper_waiting_on_a_dead_dependency_is_abandoned_not_blocked` (`agentdesk/graph.rs`),
   `cancel_wakes_a_waiter` (`airun/cli_run.rs`, alongside a comment recording that a
   previous review argued from prose that Stop could not reach the audit phase and the
   tests showed otherwise), and three unlink tests including idempotency
   (`commands/agent_import.rs`) all pass.

   What is genuinely unproven is the WIRING, not the logic: these are pure-function tests
   over the scheduler, the cancel primitive and the store. Nobody has watched a real lead
   spawn a helper in the built app, or pressed Stop while an audit was running. That is a
   smaller and more honest claim than "only reachable in unit tests", and it is the one
   an acceptance run has to close.

   The scheduler is genuinely connected, which was worth checking rather than assuming:
   `graph::schedule` is called twice in `commands/agent_graph.rs` (around the helper
   integration path), under an integration lock, re-reading the session after any
   mutation and re-scheduling once abandoned helpers are marked. This is not another of
   the wiring gaps this project has found before, where a command existed, was
   registered, was tested and was never called.
7. The original list: dirty checkouts, two simultaneous same-repo sessions, two real
   uncommitted helpers, delete/rename/binary changes, conflict and restart, child-process
   cleanup, window focus, Split View cross-repo behaviour, scaling, keyboard and
   screen-reader use, performance.

## Open by design decision, not by omission

### Transports

Five tools, one registry (`ai/agent/registry.rs`), three protocols: ACP over stdio for
Copilot, Gemini and opencode; Codex's own app-server (JSON-RPC over stdio); Claude Code's
`--print --input-format stream-json --output-format stream-json`. The bridge packages
(`claude-agent-acp`, `codex-acp`) are gone: a tool that works in the terminal works here.

Denial is still per tool, because no protocol standardises it:

| Tool | Read-only bound | Mid-run approval reaches GitWyrm's gate |
| --- | --- | --- |
| Copilot | `--deny-tool` at launch, outranks every allow rule | Yes (ACP `session/request_permission`) |
| Gemini | `--approval-mode=plan` for the whole session | Yes, but `exit_plan_mode` is auto-allowed non-interactively; unverified whether plan mode holds |
| Claude Code | `--disallowedTools` at launch, `--permission-mode plan` for read-only | Yes, since 2026-09-02: `--permission-prompt-tool stdio` routes each prompt as a `control_request` frame that GitWyrm answers; writing runs use `--permission-mode default` plus `--tools default` so edits and commands both reach the gate. Protocol read from the CLI binary, not documented; live run blocked by an expired login on the dev machine |
| Codex | `sandbox: "read-only"` on `thread/start` | Yes (`item/*/requestApproval`); command approvals map to the edit capability because no shell capability exists in policy |
| opencode | None | Refused for read-only work by `select::choose` |

The Claude bridge is the newest and least proven row. Two ignored real-binary tests
(`claude_answers_a_real_turn`, `claude_routes_a_command_through_the_gate`) exist for the
first signed-in machine; until one has passed, treat "Claude runs commands" as a claim.

**Release blocker found 2026-09-12: the Claude gate does not fire.**

`claude_routes_a_command_through_the_gate` was run for the first time and
failed. Two separate defects, one fixed and one open.

Fixed: a writing run could not run a command at all. `--restricted` removes the
command-running tools "unless --tools names them", and GitWyrm passed
`--tools default` -- which the CLI's own help documents as "use all tools" and
which does not restore Bash. Asked directly, a restricted run with
`--tools default` reports twelve tools and no Bash. Naming any tool also
REPLACES the set, so the file tools have to be named too or the fix trades
"cannot run a command" for "cannot read a file". Both halves are now unit
tested, including that a denied tool is never named back.

Open, and a release blocker for Claude writing runs: with Bash restored, the
command runs and **no `control_request` reaches GitWyrm**. Verified by logging
every frame the CLI sends. Ruled out by experiment: `--permission-mode manual`
(a documented value, unlike `default`), `--permission-prompts host` stated
explicitly, and dropping `--restricted` entirely. The frame shapes were read
out of the 2.1.251 binary and are undocumented; the installed CLI is 2.1.260.

Traced further on 2026-09-12 and confirmed upstream. The CLI answers a host
`initialize` control request happily -- returning its command, agent and model
lists -- so the control channel itself is alive; it simply never asks. Also
ruled out: declaring `capabilities.canUseTool` or a bare `canUseTool` in that
initialize, in either shape. The protocol is undocumented for hosts that are
not the official SDK (the request to document `--input-format stream-json`
beyond the flags table was closed as not planned, claude-code#24594) and the
missing frame is filed as claude-code#34046, "CLI does not emit can_use_tool
control_request". So this is an upstream regression to track, not a GitWyrm
defect to guess a fix for.

What GitWyrm does without guessing is notice. `ClaudeConnection::gate_count`
counts permission frames, and a writing turn that ends having been asked
nothing now logs a warning naming the upstream issue -- so the silent state
stops being indistinguishable from an approved one.

Blast radius is consent, not containment: `--restricted` and the isolated
worktree still hold, and read-only runs are unaffected because they are held by
launch-flag denial before the process starts. Codex and the ACP tools still
gate. What is missing is that `RunStep::Gate` never reaches the approval card
for this provider, so Auto on Claude asks before nothing. The comments that
stated the gate as fact now state it as an intent with the evidence beside it.

**Evidence gained 2026-09-12, second pass.** Two more never-run tests were run
and both pass: `codex_answers_a_real_turn` (a real turn against a live Codex)
and `auditor_catches_a_hollow_codex_run`. The auditor one is the significant
one -- it is the acceptance test the plan called for, it had never been
executed, and it worked: told to cut a corner against a spec that asked for
more, the run was caught and sent back naming both shortfalls (a function that
always returned true, and a test that only checked one value). Item 5 of the
list below moves from claim to evidence.

With `claude_answers_a_real_turn` from the previous pass, all three providers
have now completed a real turn end to end.

The Claude consent gap is now also stated where a person can act on it. It is a
property of the tool rather than of any one run -- on 2.1.260 it is true of
every writing run -- so it is shown on the tool's own row in Agent setup, read
once before the tool is chosen, rather than as a banner on each result arriving
after the choice was made. Driven by a field on the registry row, so the note
goes quiet on its own when upstream restores the frame.

**Evidence gained 2026-10-10.** The ignored tests were run for the first time on a
machine with `copilot` 1.0.82, `claude` 2.1.260, `codex` 0.151.0 and `opencode` 1.18.10
installed. Five now pass and are recorded rather than claimed: the shell probe, skills
discovery, the provider picker rows, submodule detection, and the auditor prompt.

`lists_models_and_answers_a_prompt` failed on first run, and the failure was informative
in a way its assertion was not. It demanded `models.len() > 1` -- a claim about the
runner's GitHub plan rather than about GitWyrm -- and aborted before the half that
exercises our own code. Reshaped to assert that a list came back and to complete with
the account's own first model, it passes: a real turn reached a live provider and
answered correctly. That is the whole `complete_streaming` body, session creation, the
deny-all handler and the event pump, proved for the first time.

The thin model list it exposed is the allowlisting described under Usage and quota
below, confirmed directly: the stored token authenticates (`/user` returns 200) while
`copilot_internal/v2/token` returns 403. It also turned out to be a real product fault,
fixed separately -- see the model-list change and Q&A 469-479.

### Usage and quota

Only what a provider reports is recorded. `plan_limit` and `plan_reset_at` still have no
source: ACP exposes no quota, and GitHub returns real entitlement data only to allowlisted
OAuth apps, silently. Parity with OpenChamber's quota line means per-provider credential
scraping, which is a decision to make deliberately rather than drift into.

### Beyond v1: three packages, written 2026-09-02

These were prose here, which meant nobody else could pick them up. Each is now a real
OpenSpec change with a proposal, spec deltas, a design and tasks, and all three pass
strict validation. None is started.

| Package | What it is, and the decision that shapes it |
| --- | --- |
| `agent-desk-goal-continuation` | Keep taking turns toward a stated goal until it is met or a budget runs out. Continuation is a decision made BETWEEN turns from evidence (the auditor's verdict, a completion condition), never the agent's own claim, and it grants no authority the first turn did not have. Stop ends the goal, not one turn; a declined gate ends it too. |
| `agent-desk-remote-access` | Watch and steer a run from a phone. Both ends dial outward to a rendezvous that routes sealed bytes it cannot read: no opened port, no hosted copy of anyone's code, identity per host and account rather than per address (thousands behind one VPN address must not collide). A sleeping desktop is reported plainly and answers are refused rather than queued. Largest package, sequenced last: there is no server, protocol, auth or streaming today. |
| `agent-desk-plugin-compatibility` | Host what other agent tools already load rather than defining a GitWyrm plugin format. Show what a run will load, let one be excluded per session without editing the tool's own config, and carry one in through the existing copy path. Where a tool cannot enforce an exclusion, say so rather than showing a control that does nothing. |

The standing constraints behind the remote package (no opened ports, outward-dialling
rendezvous, thousands of hosts behind one address, phone plainly dead when the desktop
sleeps) are now requirements with scenarios rather than notes in a plan file.

### Skills and connectors

Skills are read (folders with `SKILL.md` front matter; verified against 13 real skills)
and shown, and clients are one table. A skill can be copied: the folder copy has the same
hash gating, backup and undo as a connector. Only Claude Code's skills folder is verified,
so the other clients' skill locations are still unproven.

Connectors can be copied into all five clients. Four keep their servers in JSON, merged so
every byte outside the edited member survives; Codex keeps its in TOML, merged through a
real document model so a hand-written config keeps its comments and layout.

### Snip

Backend-only (`snip_detect`, `snip_gain` in the bindings, nothing in the UI calls them).
It works by rewriting commands inside the agent process through config the agent loads
itself. Now that write-capable runs may run commands, its lever does apply to Agent Desk
on those runs; whether Copilot's hooks fire under `--acp --stdio` is still undocumented
and needs a real test.

### Plan checklist parser

Decided 2026-09-02: the Plan-mode instruction now asks the lead to write its summary as
a Markdown task list (`- [ ] step`), the exact shape `agentDeskPlan.ts` parses, and a test
pins the marker. The parser still fails safe when a model writes prose instead.

## Housekeeping the audit found

- The branch was 63 commits behind `main` with conflicts in `copilot_cli.rs` and
  `ai/complete.rs`; merged 2026-09-02.
- 63 Agent Desk OpenSpec tasks remain unchecked, and the 76-line acceptance checklist is
  entirely unchecked. The 78 unchecked tasks across the nine packages were reconciled
  against the code on 2026-09-02, one by one. Seven were done and are ticked. Of the 71
  left open, about 30 need a person in the built app or an evidence record, a handful are
  implemented but missing one specific automated test, and these are genuinely not built:

  | Package | Gap |
  | --- | --- |
  | review-and-landing | helper output and diff reachable from the graph node (2.2 **closed 2026-09-02**; 2.6 lead result from the integration worktree still open); panels refreshed after Keep/undo/commit (2.7, 3.5; **closed 2026-09-02**); Update-PR half of 4.1 **closed 2026-09-02** (editable title/body dialog, existing-PR detection by branch); Spec Desk shell removal (6.5); acceptance checklist (6.1) |
  | workspace-layout | select event carries no repo and nothing proves an already-open Desk selects (1.3) |
  | agent-graphs | durable operation queue for integration (5.1); model per node (6.2; current action and output link **closed 2026-09-02**); orphaned helper worktrees when a helper dies before a result (6.4; **closed 2026-09-02**); `ChecksPass` / `FilesChanged` completion conditions **enforced 2026-09-02** (`agentdesk/completion.rs`, judged from recorded checks and changed files before integration). Both prompts now offer `checksPass` and `filesChanged`, so a lead can ask for one. Still unproven end to end: no live run has yet produced a proposal using a stricter kind, which native acceptance should cover |
  | source-kickoffs | typed retry/reconnect cards instead of toasts (2.4, 5.3; **closed 2026-09-02**); source enrichment after the Desk is visible (3.4, 4.4); escalate a review into a fix (4.6; **closed 2026-09-02**) |
  | openspec-workflows | 4.2 **closed 2026-09-02**: a finished spec-sourced chat can draft an update to its tasks, proposal or design through the existing drafter (`openspec_draft_from_session`), landing as an unsaved edit the person saves. Legacy Spec Desk runs (1.4) **reopened**: the backend convergence is real (`ai_run_start`, `run_engine` and their worktree provisioning are deleted, and the spec view's task button opens an Agent Desk chat), but a second audit found the button silently did nothing in the spec window and a duplicate action beside it. Both are fixed; the task stays open because Spec Desk still projects the old run store (status bar, spec cards, AI tab) and because completion now means: from the real spec window, one click opens a durable chat, starts an isolated run, and leaves no legacy run state or duplicate action visible |
  | configuration-sync | per-item destination selection in batch (2.2; **closed 2026-09-02**); the Codex writer (4.1) **closed 2026-09-02**, merging into `[mcp_servers.<name>]` through `toml_edit` so comments and layout survive, with an end-to-end apply and undo against a real file. VS Code Copilot and OpenChamber (4.4, 4.5) **closed 2026-09-02**, writing into whichever server map the file already uses. Skill copying **closed 2026-09-02**: `agent_config/skill_write.rs` does the folder copy with hash gating, backup and undo, and is wired through the preview, apply and undo commands (`skill_copy_preview`, `apply_skill_destination`, `undo_skill_at`) with an end-to-end test |
  | conversation-shell | run-event links to diff/file/task (4.4; **closed 2026-09-02**) |
  | external-chat-import | OpenChamber adapter is detection-only (3.5; its Windows data path **fixed 2026-09-02** after reading OpenChamber's own `cli-paths.js`, which uses `~/.config/openchamber` everywhere); unlink (4.3) and log redaction (5.2) **closed 2026-09-02** |

  Closed the same day, each with unit tests but not yet clicked in the app: graph node
  activity, output and View changes; Fix this from a review result; import unlink and log
  redaction.

## Release order

1. Native acceptance, in the order listed above, on a machine with all three tools
   signed in.
2. Run the two ignored Claude tests on a signed-in machine; fix what they find.
3. Verify Gemini's plan mode holds across a non-interactive `exit_plan_mode`.
4. Repeat the live auditor run from the app, and against Claude once signed in.
5. Decide the quota question (per-provider scraping or none).

## Meaning of "ready"

Agent Desk is ready when a person can open a chat on any installed tool, describe a goal,
watch the agent edit, run its checks and correct itself, be asked before anything risky,
and trust that a follow-up message, a switch of chat, or a restart never loses the
conversation or leaks one chat's authority into another. The code now claims all of this;
native acceptance is what turns the claim into evidence.
