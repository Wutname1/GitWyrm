# Change: The agent room (several monitored runs at once)

## Why

Once single-task runs are trusted, the ceiling becomes throughput: a change with eight
independent tasks should not require eight sequential babysat runs. The agent room is
the Desk's future third act - several monitored runs at once, one agent per task, each
with the same run grammar (preflight, stream, gates, stop) the user already knows.

This change is deliberately a **draft**: requirements are not written yet. It exists so
the roadmap is visible in the Desk (it renders as a Draft with open spec-phase tasks)
and so the single-run work in `add-ai-task-runs` keeps its constraints
(one-state-machine, per-run session integrity) compatible with a multi-run future.

## What Changes

- (To be specified) A view hosting multiple concurrent runs on one change: per-run
  cards with state, stream access, gate answering, and individual stop buttons
- (To be specified) Task scheduling rules: which tasks may run in parallel, worktree or
  branch isolation per agent, merge/commit ordering
- (To be specified) A shared approval queue so gates from any run are answerable from
  one place

## Impact

- Affected specs: `ai-runs` (deltas to be written during the spec phase below)
- Depends on: `add-ai-task-runs`, `add-ai-agent-engine`, and `add-ai-run-completion`
  proven in real use first
