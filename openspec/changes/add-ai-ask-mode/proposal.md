# Change: Ask about this change (read-only AI chat)

## Why

Sometimes the question is "why is it built this way?", not "build it". Ask is a
read-only conversation grounded in the change package and the code. It must be
*visibly* different from a run - mode confusion ("I thought it was just answering, why
did files change?") is the most damaging failure a beginner tool can have.

## What Changes

- "Ask about this change" in the rail opens the ✦ tab in ask mode: chat bubbles, a
  read-only banner, no Stop button, provider label suffixed "· read-only"
- Answers grounded in the proposal, deltas, design, and code, citing sources as chips
  that jump to the matching Desk tab
- Escalation: asked to do work, Ask explains it cannot edit and offers starting a run
  as one explicit click
- Session rules shared with runs (one session at a time, no stale replies)

## Impact

- Affected specs: `ai-ask` (new capability)
- Affected code: ask mode of the ✦ tab, driver read-only session type
- Depends on: `add-ai-task-runs` (shares the tab, driver, session rules)
