# Change: The agent room (several monitored runs at once)

## Why

Once single-task runs are trusted, the ceiling becomes throughput: a change with eight
independent tasks should not require eight sequential babysat runs. The agent room is
the Desk's future third act - several monitored runs at once, one agent per task, each
with the same run grammar (preflight, stream, gates, stop) the user already knows.

The requirements are now written; the **build** is deliberately unscheduled. This change
stays a draft until `add-ai-task-runs`, `add-ai-agent-engine`, and
`add-ai-run-completion` are proven in real use, because a room built on a single-run
engine that has not been exercised multiplies any flaw in it by the number of agents.

## What Changes

- A view hosting multiple concurrent runs on one change: per-run cards with state,
  stream access, gate answering, and individual stop buttons, plus a room-level stop-all
- One git worktree per agent, so no two agents share a working directory and none works
  in the directory the user has open
- The user selects which tasks go in the room; independence is never inferred from task
  text, and the room says plainly that tasks touching the same files will conflict
- One commit per task, applied in the order agents finish, each carrying its task's
  `Spec:` trailer; a run whose work will not apply cleanly ends in a conflict the user
  resolves without affecting the others
- A shared approval queue so gates from any run are answerable from one place
- A plain-language cost statement before starting: how many agents, and that each uses
  the AI plan separately

## Impact

- Affected specs: `ai-runs` (deltas to be written during the spec phase below)
- Depends on: `add-ai-task-runs`, `add-ai-agent-engine`, and `add-ai-run-completion`
  proven in real use first
