# Change: The run console (UI, gates, stop, steering)

## Why

This is the heart of v3: with a provider configured, the Desk runs a task itself - the
user watches a calm, plain-language console, answers approval gates, can steer with
notes, and can stop instantly at any time. No external client required; the run is
monitored, bounded by guardrails, and always recoverable because everything is git.

This change is now the **console half** only. The engine that actually edits files and
runs commands is `add-ai-agent-engine`, and completion (validate, tick, commit review)
is `add-ai-run-completion`. Three changes rather than one because the original was 23
tasks spanning a whole agent runtime plus its entire UI - too large to verify in one
pass, and this plan has already been bitten once by committing an unverified surface.

Splitting this way also means the console can be built and verified against a scripted
driver before the engine exists, so every state a user can land in (gate open, stopped,
failed, provider expired) is exercised on screen rather than reasoned about.

## What Changes

- The ✦ AI tab in the Desk: run header (state pill, Stop), guardrail line, preflight
  checklist, activity stream, inline gate cards, steering composer with quick replies
- Gate mirroring: tab badge, rail banner, status bar, main-window spec card
- Stop / failure handling with keep / undo / restart choices
- Session integrity rules (one run at a time, no stale output leaking, header honesty)
- A `RunDriver` trait with a scripted implementation, so the console is verifiable now
  and the real engine drops in behind it

## Impact

- Affected specs: `ai-runs` (new capability)
- Affected code: Desk run tab components, rail banner, main-window card states, and the
  driver trait the engine will implement
- Depends on: `add-ai-provider-surface`
- Depended on by: `add-ai-agent-engine` (implements the trait), `add-ai-run-completion`
