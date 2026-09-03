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
| Queued messages | A message sent during a turn was saved and never read | A clean Finish with nothing else live starts the next turn with the same mode/team/tool after a note in the chat. Stopped or Failed turns never restart |
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
   get a reply. Codex and Copilot are verified by real-binary tests (`codex_answers_a_real_turn`,
   `tests/copilot_acp.rs`, both re-run 2026-09-02); Claude is blocked on the development
   machine by an expired login.
2. Send a second message while a turn is running and watch the follow-up turn start on
   its own when the first finishes, and NOT start after pressing Stop.
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

### Usage and quota

Only what a provider reports is recorded. `plan_limit` and `plan_reset_at` still have no
source: ACP exposes no quota, and GitHub returns real entitlement data only to allowlisted
OAuth apps, silently. Parity with OpenChamber's quota line means per-provider credential
scraping, which is a decision to make deliberately rather than drift into.

### Plugins and remote access

Provider protocols are a compile-time enum on purpose; `agent_config` reconciles other
tools' configuration but is not a runtime plugin framework. Remote and mobile clients need
a headless Agent Desk service (authentication, durable jobs, event streaming, capability
policy) to exist first, because today every command is an in-process Tauri call over
local child processes and filesystem sessions. Neither is started. The constraints the
user set for remote (no opened ports, outward-dialling rendezvous, thousands of hosts
behind one IP, phone plainly dead when the desktop sleeps) are recorded in the plan file
and unchanged.

### Skills and connectors

Skills are read (folders with `SKILL.md` front matter; verified against 13 real skills)
and shown, and clients are one table. Skills still cannot be copied: they are file trees
and the writers edit one JSON member. Only Claude Code's skills folder is verified.

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
  | review-and-landing | helper output and diff reachable from the graph node (2.2 **closed 2026-09-02**; 2.6 lead result from the integration worktree still open); panels refreshed after Keep/undo/commit (2.7, 3.5; **closed 2026-09-02**); Update-PR half of 4.1; Spec Desk shell removal (6.5); acceptance checklist (6.1) |
  | workspace-layout | select event carries no repo and nothing proves an already-open Desk selects (1.3) |
  | agent-graphs | durable operation queue for integration (5.1); model per node (6.2; current action and output link **closed 2026-09-02**); orphaned helper worktrees when a helper dies before a result (6.4; **closed 2026-09-02**); `ChecksPass` / `FilesChanged` completion conditions never evaluated |
  | source-kickoffs | typed retry/reconnect cards instead of toasts (2.4, 5.3; **closed 2026-09-02**); source enrichment after the Desk is visible (3.4, 4.4); escalate a review into a fix (4.6; **closed 2026-09-02**) |
  | openspec-workflows | no path from Agent Desk to the spec draft/review writer (4.2). Legacy Spec Desk runs (1.4) **closed 2026-09-02**: `ai_run_start`, `run_engine` and their worktree provisioning are deleted, and the spec view's task button opens an Agent Desk chat, so there is one execution path |
  | configuration-sync | per-item destination selection in batch (2.2); writers for Codex, VS Code Copilot and OpenChamber (4.1, 4.4, 4.5) |
  | conversation-shell | run-event links to diff/file/task (4.4; **closed 2026-09-02**) |
  | external-chat-import | OpenChamber adapter is detection-only (3.5); unlink (4.3) and log redaction (5.2) **closed 2026-09-02** |

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
