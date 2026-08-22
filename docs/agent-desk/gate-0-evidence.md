# Gate 0 evidence

Recorded 2026-08-21 against commit `726a5be`, per R0.7 of
`implementation-reset-2026-08-21.md`. No product behavior is claimed here - this only
establishes that the baseline is trustworthy enough to build on.

## R0.1 / R0.2 - ownership and dirty files

`git status --short` is **empty**. The three files the reset doc listed as dirty
(`src-tauri/src/commands/agent_desk.rs`, `src/views/AgentDeskView.tsx`,
`src/components/domain/agent-desk/SessionComposer.tsx`) were reconciled and committed in
`726a5be` together with the revised task lists. Nothing was overwritten: the in-flight
transcript work from the interrupted run was verified compiling and passing before being
committed separately in `ee9a8fb`.

## R0.3 - strict OpenSpec validation

    npx openspec validate --all --strict
    Totals: 25 passed, 0 failed (25 items)

Includes `change/add-ai-agent-engine`, which the reset doc recorded as red. It validates
clean at this commit; the spec was rewritten earlier (`9c4299c`) to describe the CLI-only
architecture that actually ships.

## R0.4 - the agent-config location test

    cargo test --lib -- --test-threads=1 agent_config::locations
    test result: ok. 3 passed; 0 failed

Including `detect_clients_reports_present_only_when_a_file_actually_exists`. This test is
**flaky under parallelism, not broken**: it was observed failing during full-suite runs
and passes consistently when run serially. Client detection was NOT weakened to make it
pass - no production code was touched. The flake is test isolation (shared filesystem
state between location tests), and fixing it properly is a real task, not a Gate 0
blocker.

## R0.5 - Rust suite, one process only

    cargo test --lib -j 2 -- --test-threads=4
    test result: ok. 1014 passed; 0 failed; 1 ignored; 0 measured

Run as a single Cargo process. Note: the default parallelism links 21 integration test
binaries at once and has repeatedly been killed (exit 137) on this machine, freezing the
desktop. **Always cap jobs.**

## R0.6 - TypeScript and frontend suite

    npm run typecheck      -> exit 0, no output
    npm run test:unit      -> Test Files 54 passed (54), Tests 652 passed (652)

## Gate 0 verdict

**PASSED.** Ownership is clean, strict OpenSpec is green, TypeScript is green, frontend
tests are green, and the Rust suite is green in a single process.

Two caveats carried forward rather than hidden:

1. The `agent_config::locations` flake under parallel test threads is real and unfixed.
2. `npm run test` is not a safe command on this machine - it chains typecheck, the
   frontend suite, 21 linked Rust test binaries, and then `tauri dev`, which never exits.
   Use `npm run tauri dev` to launch, and the capped commands above to verify.

## How verification runs from here

Sub-agents do not run `cargo` or `npm`. Several concurrent builds choke the machine.
Agents write code; verification runs serially in the main session, one build at a time.

---

# Reset progress log

## R4 - app-wide workspace (landed, unverified natively)

The Desk's identity is no longer a repository. `SessionSidebar` defaults to app-wide
(`scopeToCurrentRepo` starts false) with "This project only" as an optional narrowing, and
the pane-clearing calls that fired on every main-window repo change are gone - pane
selections, drafts and scroll positions now survive a repo switch. Per-pane repo context
was already resolved per session from `header.repoId/repoPath/repoName`; that is now
covered by tests rather than only by reading.

R4.6's native scenario (two repos, two live sessions, cross-project Split View) is NOT
verified. Unit-proven only.

## R5 - OpenSpec as execution context (wired by the main session)

The R5 agent built `render_for_prompt` (honest absent-document markers) and `fingerprint`,
but was correctly barred from touching `start_execution_at`, so nothing called them - the
exact "built but not wired" failure this reset exists to stop. The main session wired it:

- `build_prompt`'s call site now resolves the OpenSpec context for the session's source and
  prepends `render_for_prompt`'s text to the prompt actually handed to the engine.
- `ExecutionRecord` gained `context_fingerprint: Option<String>` (`#[serde(default)]`, so
  existing files load unchanged), stamped inside the SAME locked write that creates the
  record - not a second acquisition.

## R7 - imports and config sync (landed)

Two real defects closed:

1. **Secrets crossed IPC and hit disk in plaintext.** `PreviewOutcome::Ready` returned the
   whole `CopyPlan`, including `source_item.extra` and every `proposed_content` - i.e. real
   API keys - to the renderer. Now returns a `RedactedCopyPlan` that drops those fields
   entirely. The unredacted plan is still written server-side for apply to read back, so
   the apply path is unchanged and still tested end to end.
2. **"Match selected apps" wrote a batch with no preview.** It previewed and applied in one
   handler. A new `BatchReviewDialog` renders every per-item, per-destination plan first and
   only then applies, through the same hash-gated path a single copy uses.

`ImportPicker` is now reachable: a fourth centre tab ("Import chats"), mounted only in its
own branch so the adapter scan never makes chat loading wait.

## Still open at this point

- R1/R2 (`ExecutionPolicy`, cancellation registry) in flight; two compile errors in
  `agent_desk.rs:1327` and `airun.rs:782` belong to that work.
- `src/lib/bindings.ts` is stale for `RedactedCopyPlan`/`RedactedDestinationPreview` until
  Rust compiles and the exporter can run.
- R3 in flight. R6 and R8 not started.

## R3 - source-bound solo loop (landed, two follow-ups outstanding)

The flagship loop is wired. Right-click an issue -> Fix now: creates/focuses the session,
emits a targeted `agent-desk://select-session` so the Desk lands on that exact chat rather
than relying on query invalidation, and then AUTO-STARTS the execution for write-capable
intents. No second user message is required, which was R3.2's whole point.

`ResultReviewPanel` - previously imported by zero files, the clearest example of the
"built but unreachable" pattern - is now mounted in `ConversationPane` for terminal
sessions, with Keep/Undo/Revise/Commit/PR-draft wired to their existing commands.

R3.6 was handled honestly: there is no steering channel into a live ACP session, so a
message sent mid-run is saved to the transcript and labelled "saved for the next turn"
rather than pretending the running agent received it.

**Two follow-ups the R3 agent could not reach** (both inside `start_execution_at`, owned by
the concurrent R1/R2 agent at the time):

1. R3.5 is half done - `ExecutionRecord` gained `base_oid`, `provider`, `mode`, `team`, but
   nothing populates them yet.
2. R3.7 - completion/failure/stop must AUTOMATICALLY build a durable result. Until it does,
   the newly-mounted `ResultReviewPanel` will show "No result yet" even after a real run.
   This is the difference between mounted and working, and it is not done.

## Bindings owed

`src/lib/bindings.ts` is stale for: `RedactedCopyPlan`, `RedactedDestinationPreview`,
`PreviewOutcome::Ready`'s changed field type (R7), and `ExecutionRecord`'s new
`baseOid`/`provider`/`mode`/`team`/`contextFingerprint` fields (R3/R5). Regenerate once
Rust compiles - the frontend will not typecheck until then.

---

# Second audit response (2026-08-22)

## Step 1 - execution-addressed routing and launch authority (landed)

**P0-A: an event could reach the wrong chat.** `RunSessionLinks` was keyed by REPOSITORY, so
linking a second session for the same repo overwrote the first, and routing resolved the
destination from `event.repo_id`. With an app-wide sidebar and concurrent helpers, two chats
in one repository could cross-contaminate. It is now keyed by execution ID - the same
addressing `ExecutionRegistry` already used, rather than a third structure. Unlinking one
execution can no longer disturb a sibling's mapping.

The fix also surfaced a latent shadowing bug: in the `AlreadyRunning` arm the returned
`execution_id` shadowed the outer just-linked one, so the cleanup unlinked the WINNER rather
than the loser.

**P0-B: read-only was advisory.** `DENIED_TOOLS` was a single hardcoded constant
`["shell", "url"]`. `write` was never denied, `discover(cwd)` took no policy, and `connect()`
passed the same list every time - so Review and Summarize launched WITH write capability. The
engine-boundary refusal only fires when the provider chooses to ask; remembered approvals or
`--allow-all-tools` suppress the request entirely.

Now `denied_tools_for(policy, started)` appends `"write"` whenever
`policy.check_tool_capability(started, EditFile)` refuses - the SAME function the runtime check
calls, so the two layers cannot drift apart. Denial takes precedence over every allow rule
including `--allow-all-tools`, which is the property worth having. The runtime check stays as
defense in depth.

I had previously reported read-only as "enforced at the engine boundary". That was true of the
code path I read and false as a security claim, because I only verified the path where the
provider asks. The audit's word - advisory - was correct.
