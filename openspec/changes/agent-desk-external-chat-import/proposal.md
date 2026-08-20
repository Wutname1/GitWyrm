# Change: Import and continue chats from external agent clients

## Why

Users already have useful sessions in Codex, Claude Code, OpenCode, VS Code Copilot, and
OpenChamber. Agent Desk should provide continuity without editing or pretending to own
those clients' histories.

## What Changes

- Add versioned read-only client adapters and failure-isolated discovery.
- Import external sessions as provenance-preserving conversation segments.
- Reconcile project paths to GitWyrm repositories.
- Support Continue here and Continue externally when the client allows it.
- Ship adapters separately behind capabilities.

## Impact

- New `src-tauri/src/agentdesk/adapters/` and imported-session UI states.
- Reads external client data only; no external history writes.
