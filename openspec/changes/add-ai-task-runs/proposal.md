# Change: In-app AI task runs (the run engine and console)

## Why

This is the heart of v3: with a provider configured, the Desk runs a task itself - the
user watches a calm, plain-language console, answers approval gates, can steer with
notes, and can stop instantly at any time. No external client required; the run is
monitored, bounded by guardrails, and always recoverable because everything is git.

This change covers the run lifecycle up to "all steps done". Completion (validate,
tick, commit review) ships separately in `add-ai-run-completion` so each half is
independently testable.

## What Changes

- Backend run driver: sessions against the configured provider, working on the linked
  branch, streaming typed events (preflight, plan, edit, check, gate, note, error)
- The ✦ AI tab in the Desk: run header (state pill, Stop), guardrail line, preflight
  checklist, activity stream, inline gate cards, steering composer with quick replies
- Gate mirroring: tab badge, rail banner, status bar, main-window spec card
- Stop / failure handling with keep / undo / restart choices
- Session integrity rules (one run at a time, no stale output leaking, header honesty)

## Impact

- Affected specs: `ai-runs` (new capability)
- Affected code: new `src-tauri` run-driver module, Desk run tab components, rail
  banner, main-window card states
- Depends on: `add-ai-provider-surface`, `add-spec-commit-links` (linked branch)
