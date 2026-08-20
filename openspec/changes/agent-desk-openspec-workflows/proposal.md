# Change: Make OpenSpec a first-class Agent Desk source

## Why

Agent Desk replaces Spec Desk without losing its strongest planning layer. A session
started from a change or exact task must keep proposal, design, deltas, tasks, progress,
and file-backed completion aligned.

## What Changes

- Add OpenSpec change/task source variants and context builder.
- Map current Spec Desk selections/runs into Agent Desk sessions.
- Let Plan draft a graph tied to requirements and tasks.
- Reuse existing OpenSpec writers for task/spec updates.
- Preserve all no-AI editing and handoff paths.

## Impact

- Extends current `openspec` parser/writer and Agent Desk source rendering.
- Migrates current `DeskDetail`, `DeskActionRail`, and run tab behavior without shadow data.
