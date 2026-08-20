# Tasks

## 1. Inventory

- [ ] 1.1 Define skill, MCP connector, configuration location, secret reference, difference,
      destination, plan, operation receipt, and typed warning models.
- [ ] 1.2 Read configuration through adapters with no write capability in scan commands.
- [ ] 1.3 Normalize identity while retaining raw client-specific fields and source path.
- [ ] 1.4 Mark same/different/missing/unsupported/conflict per item and destination.
- [ ] 1.5 Add redaction tests for tokens, environment values, headers, and command arguments.

## 2. Agent Setup UI

- [ ] 2.1 Add inventory table/list with source, per-client state, and filters by item kind.
- [ ] 2.2 Let the user choose one item and one or more destinations.
- [ ] 2.3 Show exact destination files, semantic changes, warnings, and secret handling.
- [ ] 2.4 Keep Match selected apps as a batch of visible per-item plans, not a hidden overwrite.
- [ ] 2.5 Show immediate pending/success/failure and operation receipt with Undo.

## 3. Safe write framework

- [ ] 3.1 Build preview command containing before hash and proposed content.
- [ ] 3.2 Refuse apply when destination hash differs from preview.
- [ ] 3.3 Write backup and receipt before temp + flush + atomic rename.
- [ ] 3.4 Implement Undo with current-hash conflict detection.
- [ ] 3.5 Preserve file encoding, line endings, comments, ordering, and unknown fields where
      the client's format supports them.

## 4. Client writers

- [ ] 4.1 Codex merge writer and fixtures.
- [ ] 4.2 Claude Code merge writer and fixtures.
- [ ] 4.3 OpenCode merge writer and fixtures.
- [ ] 4.4 VS Code Copilot writer only for documented safe settings surfaces.
- [ ] 4.5 OpenChamber writer only where its schema is independently proven.
- [ ] 4.6 Keep read-only inventory when a writer is unsupported.

## 5. Proof

- [ ] 5.1 Test concurrent destination edit refusal and byte-identical Undo.
- [ ] 5.2 Test partial batch failure leaves completed receipts and untouched failed targets.
- [ ] 5.3 Test backup recovery after simulated replacement failure.
- [ ] 5.4 Test that secrets never appear in UI snapshots or normal logs.
- [ ] 5.5 Record Gate 7 separately per client writer.
