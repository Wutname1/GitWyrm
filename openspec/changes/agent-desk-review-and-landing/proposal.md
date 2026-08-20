# Change: Complete Agent Desk review, landing, and cleanup

## Why

An agent session is not useful until its result can be reviewed, kept, revised, committed,
or discarded without losing work. Existing completion/diff/worktree behavior must become
one coherent Agent Desk finish flow.

## What Changes

- Add a unified result state over existing diffs, checks, commits, and worktrees.
- Review helper and combined results in GitWyrm's existing diff UI.
- Reuse keep/undo/revise and intentional commit behavior.
- Make PR creation a separate explicit handoff and never push implicitly.
- Finish migration/cleanup only after restart/accessibility/performance proof.

## Impact

- Integrates existing `add-ai-run-completion`, diff viewer, worktrees, host actions, and
  OpenSpec trailers with session results.
