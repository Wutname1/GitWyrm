# Design

Adapters expose detection, list, read, config read, and optional external continuation.
They are version-gated and fixture-driven. Import stores original client/session/message
IDs and timestamps. Re-import deduplicates by adapter plus external IDs and cursor.

Imported messages are grouped in an external segment. Continue here starts a new native
segment with a generated handoff summary, original transcript still intact. Continue
externally launches only a documented/supported mechanism and never claims the external
client accepted context when it merely opened.

Adapter order: Codex, Claude Code, OpenCode, VS Code Copilot, OpenChamber.
