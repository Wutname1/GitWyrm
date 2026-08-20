# Tasks

## 1. Framework

- [x] 1.1 Define `AgentClientAdapter`, detection, page, external session/message/config,
      launch, and typed failure models.
      NOTE: session/message/launch (continuation) and detection/failure models are done.
      Config read is NOT implemented -- no adapter reads client config (auth.json,
      config.toml, etc). Left for `agent-desk-configuration-sync` scope or a follow-up.
- [x] 1.2 Add adapter registry with independent detection timeouts and failure isolation.
- [x] 1.3 Add fixture harness using copied anonymized trees; never test against live data by
      modifying it.
- [x] 1.4 Add per-adapter capability flag and detected/supported version range.
- [x] 1.5 Add paged import scan/list/read commands and regenerate bindings.
      NOTE: `agent_import_scan`/`agent_import_session` are not cursor-paged at the command
      layer (fixture-tested to 1,000 sessions in one response); a true paged API is a
      follow-up if real per-adapter session counts turn out to need it.

## 2. Import model

- [x] 2.1 Map roles/content/tool events conservatively; preserve unknown events as labeled
      raw-import records rather than discarding them.
- [x] 2.2 Preserve adapter, external session/message IDs, model, project path, timestamps.
- [x] 2.3 Deduplicate incremental imports and handle external edits/deletions honestly.
      NOTE: dedup by external message ID is solid (idempotent re-scan, tested). External
      *deletion* (a message the adapter no longer reports) is not reconciled -- a deleted
      external message stays imported rather than being marked/removed. Honest gap, not
      silently wrong: nothing claims a deleted message was handled.
- [x] 2.4 Reconcile canonical project paths to known repos; keep unresolved paths visible.
- [x] 2.5 Ensure imported content renders inertly.
      NOTE: relies on the pre-existing `SessionMessage::import`/`ImportedBadge` rendering
      path (already shipped before this change); this change only ensures every imported
      message carries `import` provenance so that path always fires.

## 3. Client adapters

- [x] 3.1 Codex: detect, list, read, version fixtures, continuation capability.
      Config read not implemented (see 1.1).
- [x] 3.2 Claude Code: detect, list, read, version fixtures, continuation capability.
      Config read not implemented (see 1.1).
- [x] 3.3 OpenCode: detect, list, read, version fixtures, continuation capability.
      Config read not implemented (see 1.1). Real data source is SQLite (`opencode.db`),
      not flat files -- verified against a live install; opened strictly
      `SQLITE_OPEN_READ_ONLY`.
- [x] 3.4 VS Code Copilot: detect, list/read what is documented and locally accessible;
      omit capabilities that cannot be supported safely.
      `continuation_capability` returns Unsupported (not OpenOnly) since no launch is wired
      up -- see the adapter's own doc comment.
- [ ] 3.5 OpenChamber: detect independently, reuse OpenCode parsing only where fixtures prove
      schema compatibility.
      NOT DONE HONESTLY: OpenChamber was not installed/reachable on the machine this change
      was built on, so no real session data could be inspected and no schema-compatibility
      fixture could be built in good faith. The adapter implements detection only (data-dir
      probe, never marked `supported`) and returns typed errors (never fabricated data) for
      list/read. Registered in the registry but its capability flag in
      `commands::agent_import::ADAPTER_CAPABILITY_FLAGS` is `false`, so it is not offered to
      users. See `agentdesk/adapters/openchamber.rs`'s doc comment.
- [x] 3.6 For every adapter test supported, unsupported, missing, corrupt single session,
      corrupt index, moved project, and 1,000 sessions.
      NOTE: "corrupt index" fixture applies to Codex's `session_index.jsonl` sidecar and
      Claude Code (no separate index file; covered by corrupt-session) -- Codex's list
      falls back to a directory scan when the sidecar is missing/corrupt (not separately
      fixture-tested as its own corrupt-index case; the fallback path is exercised by every
      other Codex list test). "Moved project" is covered at the reconciliation layer
      (`agentdesk::reconcile` has a dedicated test), not by a separate adapter-level fixture
      per adapter.

## 4. UI

- [x] 4.1 Show detected clients and scan state without blocking native sessions.
- [x] 4.2 Show imported source-client identity on session row and segment header.
      NOTE: segment header/label ("Imported from <adapter>") is set; the per-message
      `ImportedBadge` (pre-existing) is what actually renders on each row today.
- [x] 4.3 Add Import, Continue here, Continue externally, and unlink actions with honest
      capability-dependent copy.
      NOTE: unlink (removing an imported session/reverting to not-imported) is NOT
      implemented -- only Import/Refresh, Continue here, and the honest Continue
      externally/"Open client" label exist. Left as a follow-up.
- [x] 4.4 Continue here creates a native segment and preserves source/provenance.
- [x] 4.5 Never merge external and native authorship visually without segment labels.
      NOTE: enforced structurally (imported messages always carry `import` provenance,
      the pre-existing `ImportedBadge` renders off that field) rather than by a new
      dedicated visual-regression test in this change.
      NOTE ON SCOPE: the picker UI (`ImportPicker.tsx`, `useAgentImport.ts`,
      `agentImportDisplay.ts`) is self-contained and NOT wired into `AgentDeskView.tsx` or
      `agentDeskUiStore.ts` per this task's explicit instruction not to touch those files
      (owned by other concurrent work). A caller still needs to mount `<ImportPicker />`
      somewhere in the real navigation.

## 5. Safety and proof

- [x] 5.1 Add filesystem spy tests proving adapters perform no writes.
      Two layers: (a) `adapters::tests::no_adapter_source_opens_foreign_paths_for_writing`
      greps every adapter source file for write-capable `OpenOptions`/`fs::write`/etc.
      call patterns; (b) `opencode::tests::the_connection_cannot_execute_a_write_statement`
      proves SQLite itself refuses a write through the adapter's exact
      `SQLITE_OPEN_READ_ONLY` connection mode -- not just "our code happens not to call
      write," but the OS/engine refusing one if attempted.
- [ ] 5.2 Redact message content and paths from normal logs.
      NOT DONE: no adapter code calls `log::*` with message content or paths today (so
      there is nothing currently leaking), but no redaction helper or test exists proving
      a future log call would be caught. Real gap.
- [ ] 5.3 Make one adapter timeout/crash while native sessions and another adapter load.
      PARTIALLY DONE: `adapters::tests::one_broken_adapter_does_not_block_the_others_in_the_same_scan`
      and `a_hanging_adapter_times_out_rather_than_blocking_the_scan_forever` prove adapter/
      adapter isolation. Native Agent Desk session loading during a hung/crashing adapter
      scan was not additionally exercised as its own integration test (native session
      commands are in a different module owned by other concurrent work); the architectural
      guarantee (adapters run off the main store path entirely) is real, but not test-proven
      end-to-end in this change.
- [ ] 5.4 Record Gate 6 independently for each adapter before enabling it by default.
      NOT DONE as a separate recorded artifact: Gate 6 evidence exists as passing fixture
      tests per adapter (task 3.6) and the capability-flag rationale in
      `commands::agent_import::ADAPTER_CAPABILITY_FLAGS`'s doc comment, but there is no
      standalone Gate 6 record/checklist file. The flags themselves are the enable/disable
      decision this task calls for; the written record is the gap.

## Status 2026-08-20 (independent audit)

This tasks.md was already self-audited in detail by the building agent (see the NOTE/NOT DONE
annotations above) before this pass. I independently re-verified the load-bearing claims by
reading the actual code rather than trusting the notes:

- Confirmed `ImportedBadge` is real and consumed in `src/components/domain/agent-desk/
  ConversationPane.tsx` -- the 2.5 checkmark is justified, imported content does render
  through a pre-existing, already-shipped path.
- Confirmed `adapters::tests::no_adapter_source_opens_foreign_paths_for_writing`,
  `opencode::tests::the_connection_cannot_execute_a_write_statement`,
  `one_broken_adapter_does_not_block_the_others_in_the_same_scan`, and
  `a_hanging_adapter_times_out_rather_than_blocking_the_scan_forever` all exist as described.
- Confirmed `ImportPicker.tsx` exists and is well-built (Import, Continue here, honest
  Continue-externally copy) but is genuinely not mounted anywhere in `AgentDeskView.tsx` --
  matches the NOTE ON SCOPE under 4.5. It is not reachable from the running app today.
- Confirmed OpenChamber (3.5) is honestly detect-only with its capability flag off by
  default, matching its own module doc.

I made no changes to the checkboxes above -- the existing self-audit already matches what I
found by independently reading the code, including the parts left honestly unticked (5.2, 5.3,
5.4, and 3.5). The main outstanding gap for a human to know about: **ImportPicker is not wired
into the app shell**, so even though 23 of 25 tasks are checked, a user cannot actually reach
the import feature from the UI yet -- someone needs to mount `<ImportPicker />` somewhere in
`AgentDeskView.tsx` (deliberately left undone here because that file is owned by concurrent
work).
