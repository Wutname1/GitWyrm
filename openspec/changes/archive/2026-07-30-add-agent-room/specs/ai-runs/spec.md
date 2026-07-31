# ai-runs Spec Delta

## ADDED Requirements

### Requirement: The agent room

The Spec Desk SHALL support several monitored AI runs at once on a single change - one
agent per task - each using the same run grammar as single runs (visible preflight,
plain-language stream, approval gates, review-first completion) and each with its own
always-visible stop control, plus a room-level stop-all.

#### Scenario: Two tasks in parallel

- WHEN the user starts agents on two independent tasks of one change
- THEN each run shows its own state, stream, and stop control
- AND stopping one never affects the other

#### Scenario: Same trust rules

- WHEN any agent in the room hits a gated side effect
- THEN that run pauses on a normal approval gate while the others continue
- AND no agent may push, exactly as with single runs

### Requirement: One worktree per agent

Each agent in the room SHALL work in its own git worktree on its own branch. Agents
SHALL NOT share a working directory, and no agent SHALL work in the directory the user
has open.

Two agents editing one working tree would interleave writes to the same files, and
afterwards neither the user nor the runs could tell whose change was whose. Worktrees
give each agent a real checkout at the cost of disk, and git already understands them,
so no custom isolation has to be invented or trusted.

#### Scenario: Isolation is real

- WHEN two agents both edit files while running
- THEN each edit lands only in that agent's worktree
- AND the user's own working directory is untouched by either

#### Scenario: Worktree cleanup

- WHEN a run ends, is stopped, or fails
- THEN its worktree is removed once its work has been merged or discarded
- AND a worktree is never removed while it holds the only copy of an agent's work

### Requirement: Only independent tasks run in parallel

The room SHALL start agents only on tasks the user has selected together, and SHALL
state plainly that tasks touching the same files will conflict. Independence SHALL NOT
be inferred from task text.

Task wording does not say what a task will touch, and a wrong guess produces exactly
the conflict the room exists to avoid. The user picks what goes in the room; the room's
job is to make that choice informed, not to make it for them.

#### Scenario: Picking tasks

- WHEN the user opens the room
- THEN they choose which open tasks to run, and the room says that tasks touching the
  same files will conflict

#### Scenario: One task is still one run

- WHEN the user selects a single task
- THEN it runs as an ordinary single run rather than opening a room

### Requirement: One commit per task, ordered by completion

Each agent's work SHALL become its own commit carrying that task's `Spec:` trailer.
Commits SHALL be applied in the order agents finish, not the order they started.

#### Scenario: Two agents finish

- WHEN agent B finishes before agent A
- THEN B's work is committed first and A's is applied on top of it
- AND each commit names the task it came from

#### Scenario: Conflict when applying

- WHEN an agent's work cannot be applied cleanly on top of what already landed
- THEN that run ends in a conflict the user resolves, the other runs are unaffected,
  and nothing is committed for it until they do

### Requirement: One approval queue for the room

Gates from any run SHALL be answerable from one place, each naming the run and task it
came from. Answering a gate SHALL affect only the run that raised it.

#### Scenario: Two gates at once

- WHEN two agents both pause for approval
- THEN both appear in the queue, each naming its own task
- AND answering one leaves the other waiting

### Requirement: The room says what it will cost before it starts

Before starting, the room SHALL state in plain language how many agents will run and
that each one uses the user's AI plan separately.

Parallel runs multiply spend in a way a single run does not, and finding that out
afterwards is the kind of surprise that loses trust permanently.

#### Scenario: Starting four agents

- WHEN the user selects four tasks
- THEN the room says four agents will run and that each uses their AI plan separately,
  before anything starts
