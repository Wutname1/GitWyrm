# Design

## Graph shape

First release supports a DAG with one root lead and at most three concurrent helpers.
Helpers cannot spawn helpers. Validation rejects cycles, missing dependencies, duplicate
node IDs, and jobs without completion conditions.

## Responsibility

The lead owns the user conversation, source, plan, helper jobs, integration, and final
answer. Helpers write only in their own worktrees and report typed outputs/events.

## Plan and Auto

- Plan persists AwaitingStart and waits for Start.
- Auto may start only after policy/DAG/worktree validation.
- Solo never constructs a graph.

## Integration

Results integrate in completion order through existing commit/completion plumbing. A
conflict pauses that integration, preserves both copies, and does not stop unrelated
helpers. Existing approval gates are keyed by helper execution ID.
