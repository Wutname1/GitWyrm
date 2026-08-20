# Change: Add lead-and-helper agent graphs

## Why

Future coding work is one conversation with a capable lead that can split bounded work
across smaller helpers. The graph must be visible, stoppable, isolated, and recoverable.

## What Changes

- Add persisted proposed/running graph types over the existing run engine.
- Support Plan approval and Auto start with one lead plus up to three helpers.
- Give every helper a marked worktree, branch, budget, path allowance, and execution ID.
- Add graph projection, per-agent stop, labeled Stop all, approvals, integration conflicts,
  and restart recovery.

## Impact

- Extends `src-tauri/src/airun/`, current worktree provisioning, session events, and the
  Agent Desk Graph panel.
