# Change: Desk actions - handoffs, spec check, archive, new blank change

## Why

This is the no-AI daily-driver loop from the mockup: compose a perfectly-scoped handoff
for any external tool, run the spec check with visible results, archive a finished
change for real, and scaffold a new blank change. Everything here must work with zero
AI configured - it is also the fallback every AI feature degrades to.

## What Changes

- Action rail: next-task handoff card (Copy task handoff / Open in opencode /
  Open in VS Code) and the handoff preview
- Handoff composer with editable templates in Settings → Specs
- "Run spec check" (openspec validate) with inline pass/warning results
- Archive flow: gated until done, plain confirm, real merge + move, counts update
- "＋ New" blank-change scaffold (user-named folder, template files)

## Impact

- Affected specs: `spec-desk-actions` (new capability)
- Affected code: Desk action rail components, `src/lib/` handoff composer, Settings view
- Depends on: `add-spec-desk-detail`, foundation CLI integration
