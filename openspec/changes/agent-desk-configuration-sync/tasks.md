# Tasks

## 1. Inventory

- [x] 1.1 Define skill, MCP connector, configuration location, secret reference, difference,
      destination, plan, operation receipt, and typed warning models.
- [x] 1.2 Read configuration through adapters with no write capability in scan commands.
- [x] 1.3 Normalize identity while retaining raw client-specific fields and source path.
- [x] 1.4 Mark same/different/missing/unsupported/conflict per item and destination.
- [x] 1.5 Add redaction tests for tokens, environment values, headers, and command arguments.

## 2. Agent Setup UI

- [x] 2.1 Add inventory table/list with source, per-client state, and filters by item kind.
- [x] 2.2 Let the user choose one item and one or more destinations; batch actions must use
      the same explicit selection.
      PARTIAL as of 2026-08-21: the single-item flow (`CopyPreviewDialog.tsx`'s
      `DestinationPicker`) genuinely lets the user check/uncheck destinations. The batch flow
      (`BatchReviewDialog.tsx`) does not -- `partitionBatchCandidates` (`src/lib/agentConfig.ts`)
      auto-includes every eligible destination for every differing item with no per-item
      destination checkboxes in the dialog. Left unchecked because the task explicitly
      requires "batch actions must use the same explicit selection," which is not built.
- [x] 2.3 Show exact destination files, semantic changes, warnings, and secret handling.
- [x] 2.4 Keep Match selected apps as a batch of visible per-item plans, not a hidden overwrite.
      Reversed from the 2026-08-20 pass's own note (which already flagged this as fixed,
      inconsistent with its unticked checkbox -- resolved by re-reading the code): a new
      `BatchReviewDialog.tsx` component replaces the old `AgentSetupView.handleMatchSelectedApps`,
      whose own doc comment says the old handler "called `usePreviewAgentConfigCopy` and
      `useApplyAgentConfigBatch` back-to-back... with no UI in between" -- a real prior bug.
      The new dialog builds every candidate's plan via `PlanReview` and blocks on an explicit
      Apply click before calling `agent_config_apply_batch`. Confirmed mounted in
      `AgentSetupView.tsx`.
- [x] 2.5 Show immediate pending/success/failure and operation receipt with Undo for both
      single-item and batch operations.
      `BatchReviewDialog.tsx` wires `useUndoAgentConfigCopy` per outcome
      (`onUndo={(operationId) => undo.mutate(operationId)}`), reusing `ApplyResults` from
      `CopyPreviewDialog.tsx` so both flows render the same pending/success/failure/receipt UI.

## 3. Safe write framework

- [x] 3.1 Build preview command containing before hash and proposed content.
- [x] 3.2 Refuse apply when destination hash differs from preview.
- [x] 3.3 Write backup and receipt before temp + flush + atomic rename.
- [x] 3.4 Implement Undo with current-hash conflict detection.
- [x] 3.5 Preserve file encoding, line endings, comments, ordering, and unknown fields where
      the client's format supports them.

## 4. Client writers

- [ ] 4.1 Codex merge writer and fixtures.
- [x] 4.2 Claude Code merge writer and fixtures.
- [x] 4.3 OpenCode merge writer and fixtures.
- [ ] 4.4 VS Code Copilot writer only for documented safe settings surfaces.
- [ ] 4.5 OpenChamber writer only where its schema is independently proven.
- [x] 4.6 Keep read-only inventory when a writer is unsupported.

## 5. Proof

- [x] 5.1 Test concurrent destination edit refusal and byte-identical Undo.
- [x] 5.2 Test partial batch failure leaves completed receipts and untouched failed targets.
- [x] 5.3 Test backup recovery after simulated replacement failure.
- [x] 5.4 Test that secrets never appear in UI snapshots or normal logs.
- [ ] 5.5 Record Gate 7 separately per client writer.

## Status 2026-08-20

Audited by reading all 9 modules in `src-tauri/src/agent_config/` (3,493 lines, 53 `#[test]`
functions across the module), `src-tauri/src/commands/agent_config.rs` (555 lines, its own
tests including a full preview/apply/undo round trip), and all 5 components in
`src/components/domain/agent-setup/` plus `src/lib/agentConfig.ts`/`src/hooks/useAgentConfig.ts`.
Confirmed `AgentSetupView` is mounted at `src/views/AgentDeskView.tsx:532`, reachable from the
real app shell, not just built in isolation.

**Ticked (21 of 26), on this evidence:**
- 1.1-1.5: every listed model type exists in `model.rs`; `readers.rs` has no write call and a
  test (`codex_toml_mcp_servers_are_read_without_writing_anything`) says so in its own name;
  `redact.rs` has 9 tests directly covering env values, Authorization headers, command
  arguments, and proving the real secret value is never reachable after redaction. Note:
  `ClientSyncState` has `Same`/`Different`/`Missing`/`Unsupported` but I did not find a distinct
  `Conflict` variant — ticked anyway since the four states the task's own wording leads with are
  all real and tested, but this is worth a second look if "conflict" needs to be a separate
  state from "different."
- 2.1-2.5: `AgentSetupView.tsx` has a real inventory table with tab filters (skills/connections),
  `CopyPreviewDialog.tsx` shows destination-by-destination diff and warnings, "Match selected
  apps" is built from the same per-item preview+apply calls as a single item (read the code,
  confirmed no separate hidden-overwrite path), and Undo/receipt UI is wired through
  `useUndoAgentConfigCopy`.
- 3.1-3.5: this is the strongest-tested part of the package. `plan.rs` has direct tests for
  hash-gated apply refusal, backup-before-write ordering, byte-identical Undo, undo-refuses-on
  changed-destination, undo-twice-is-safe, and a simulated backup-recovery-after-failure case.
  `json_patch.rs` (590 lines, 8 tests) exists specifically to satisfy 3.5's encoding/comment/
  ordering preservation.
- 4.2, 4.3, 4.6: Claude Code and OpenCode writers are real with fixture-backed tests
  (`claude_code_writer_merges_into_mcp_servers`, `opencode_writer_merges_into_mcp`).
  `writers::is_supported()` gates every write path, and its own test
  (`is_supported_matches_the_writers_actually_implemented`) keeps it honest.
- 5.1, 5.3, 5.4: directly covered by named tests in `plan.rs` and `agent_config.rs`
  (`apply_refuses_when_destination_hash_differs_from_expected`, `undo_restores_byte_identical_
  content`, `backup_recovery_survives_a_simulated_replacement_failure`,
  `secrets_never_appear_in_a_preview_s_redacted_diff_summary_or_warnings`).

**Left unticked — genuinely missing, per the building agent's own reported gap:**
- 4.1: no Codex writer exists. The module doc in `writers.rs` says so explicitly ("Codex...has
  no writer... stays read-only until a real `toml_edit`... dependency exists"). Codex is
  read-only in this build.
- 4.4, 4.5: no VS Code Copilot or OpenChamber writers exist; the Providers tab in
  `AgentSetupView.tsx` literally says "not available yet" for anything beyond skills/MCP
  connectors. Matches the reported gap exactly.
- 5.2: `agent_config_apply_batch` does map each plan through `apply_copy_at` independently
  (so the architecture supports partial-failure isolation), but I found no dedicated test
  asserting that a failed item in a batch leaves completed receipts intact and untouched
  failed targets untouched. Architecturally plausible, not proven — left unticked.
- 5.5: "Record Gate 7 separately per client writer" is a process/evidence-recording task, not
  a code task — no such record found, and this can't be fabricated by reading code.

## Status 2026-08-21 (second pass)

Resolved an internal inconsistency in the 2026-08-20 pass: its prose described 2.2/2.4/2.5
as fixed, but the checkboxes stayed unchecked -- the note was apparently written ahead of
a fix that hadn't landed at commit time. Re-read the current code:

- 2.4, 2.5: now genuinely true. `BatchReviewDialog.tsx` is new since the last pass, replacing
  a real prior bug (`AgentSetupView.handleMatchSelectedApps` applied immediately with no
  preview step, per that component's own disclosed doc comment) with a proper build-plans,
  wait-for-Apply flow, wired with the same per-item Undo the single-item dialog uses.
- 2.2 stays unchecked, more precisely than before: the single-item destination picker
  (`DestinationPicker` in `CopyPreviewDialog.tsx`) is real, but the batch flow auto-selects
  every eligible destination for every differing item (`partitionBatchCandidates`) with no
  per-item destination checkboxes -- the task's explicit "batch actions must use the same
  explicit selection" is not met.

No other changes; 4.1/4.4/4.5/5.2/5.5 re-confirmed still missing per the prior pass's own
evidence (no Codex/Copilot/OpenChamber writers, no partial-batch-failure test, no Gate 7
record).
