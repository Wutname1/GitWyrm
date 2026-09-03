# Agent Desk Spec Delta

## ADDED Requirements

### Requirement: A goal states what finishing means

A session MAY carry one goal describing what done means, taken from its source (an
OpenSpec task's text, an issue's body) or written by the person. The goal SHALL be
durable, visible in the conversation while it is active, and editable only between
turns, never while one is running.

#### Scenario: A goal is shown while it runs

- WHEN a session has an active goal
- THEN the conversation shows the goal, the turns spent, and the budget remaining

#### Scenario: A session with no goal behaves as it does today

- WHEN a session has no goal
- THEN a turn ends when it ends, and nothing starts another

### Requirement: Continuation is judged, not assumed

After a turn ends cleanly, the goal SHALL be judged against evidence the run produced:
the auditor's verdict on the real diff, and any completion condition attached to the
work. A goal judged met SHALL end the run. A goal judged unmet SHALL start one further
turn carrying what the previous turn learned.

The judgement SHALL NOT be the agent's own claim that it is finished.

#### Scenario: Hollow work does not count as met

- WHEN a turn ends and the auditor reports the work is not really finished
- THEN the goal is not met and the next turn is given the auditor's reasons

#### Scenario: Met goal stops cleanly

- WHEN a turn ends and the goal is judged met
- THEN no further turn starts and the run reports the goal as met

### Requirement: Continuing never widens authority

A continuation turn SHALL run under the same intent, mode, team, provider, worktree and
approval gates as the turn before it. Continuation SHALL NOT grant a tool, a path, or a
capability that the first turn did not have.

#### Scenario: A read-only chat cannot continue into writing

- WHEN a read-only session has a goal
- THEN continuation turns remain read-only and no file is changed

### Requirement: A person's refusal ends the goal

The goal SHALL end immediately, without a further turn, when the person presses Stop,
declines an approval gate, or the run ends as failed, stopped or refused. Stop SHALL end
the whole goal rather than one turn, and SHALL say so.

#### Scenario: Stop ends everything

- WHEN a person presses Stop during a continued run
- THEN the current turn stops and no further turn starts

#### Scenario: A declined gate is not retried

- WHEN a person declines an approval gate
- THEN the goal ends rather than starting a turn that would ask again

### Requirement: A goal is bounded before it starts

Every goal SHALL carry a turn ceiling and a time ceiling, both shown before the goal
starts and while it runs. Reaching either SHALL end the goal, reported as out of budget
rather than as finished.

#### Scenario: Out of budget is not success

- WHEN a goal reaches its turn ceiling without being judged met
- THEN the run ends reporting the goal was not met and what was spent

### Requirement: Every continuation is accounted for

The cost of each continuation turn SHALL be recorded on its own execution and rolled
into the session total, so the price of a long goal is visible while it accrues rather
than after it.

#### Scenario: Cost is visible during the run

- WHEN a goal has run several turns
- THEN the usage panel shows the total spent across those turns
