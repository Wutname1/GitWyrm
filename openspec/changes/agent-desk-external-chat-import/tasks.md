# Tasks

## 1. Framework

- [ ] 1.1 Define `AgentClientAdapter`, detection, page, external session/message/config,
      launch, and typed failure models.
- [ ] 1.2 Add adapter registry with independent detection timeouts and failure isolation.
- [ ] 1.3 Add fixture harness using copied anonymized trees; never test against live data by
      modifying it.
- [ ] 1.4 Add per-adapter capability flag and detected/supported version range.
- [ ] 1.5 Add paged import scan/list/read commands and regenerate bindings.

## 2. Import model

- [ ] 2.1 Map roles/content/tool events conservatively; preserve unknown events as labeled
      raw-import records rather than discarding them.
- [ ] 2.2 Preserve adapter, external session/message IDs, model, project path, timestamps.
- [ ] 2.3 Deduplicate incremental imports and handle external edits/deletions honestly.
- [ ] 2.4 Reconcile canonical project paths to known repos; keep unresolved paths visible.
- [ ] 2.5 Ensure imported content renders inertly.

## 3. Client adapters

- [ ] 3.1 Codex: detect, list, read, config, version fixtures, continuation capability.
- [ ] 3.2 Claude Code: detect, list, read, config, version fixtures, continuation capability.
- [ ] 3.3 OpenCode: detect, list, read, config, version fixtures, continuation capability.
- [ ] 3.4 VS Code Copilot: detect, list/read what is documented and locally accessible;
      omit capabilities that cannot be supported safely.
- [ ] 3.5 OpenChamber: detect independently, reuse OpenCode parsing only where fixtures prove
      schema compatibility.
- [ ] 3.6 For every adapter test supported, unsupported, missing, corrupt single session,
      corrupt index, moved project, and 1,000 sessions.

## 4. UI

- [ ] 4.1 Show detected clients and scan state without blocking native sessions.
- [ ] 4.2 Show imported source-client identity on session row and segment header.
- [ ] 4.3 Add Import, Continue here, Continue externally, and unlink actions with honest
      capability-dependent copy.
- [ ] 4.4 Continue here creates a native segment and preserves source/provenance.
- [ ] 4.5 Never merge external and native authorship visually without segment labels.

## 5. Safety and proof

- [ ] 5.1 Add filesystem spy tests proving adapters perform no writes.
- [ ] 5.2 Redact message content and paths from normal logs.
- [ ] 5.3 Make one adapter timeout/crash while native sessions and another adapter load.
- [ ] 5.4 Record Gate 6 independently for each adapter before enabling it by default.
