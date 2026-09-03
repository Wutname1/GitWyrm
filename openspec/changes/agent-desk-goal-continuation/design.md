# Design: goal continuation

## The decision this package makes

Continuing is a decision made *between* turns, from evidence the last turn produced. It
is not a longer turn, not a bigger budget, and not a new authority. That framing decides
almost everything else here.

## Where it hooks in

`commands::airun::route_to_agent_desk` already does exactly this shape of work: when an
execution reaches a terminal state it decides whether to start another turn, and today
it does so for a message the person sent while the agent was busy
(`queued_follow_up_for`). Goal continuation is a second reason to reach the same
conclusion, and belongs beside it rather than in a parallel mechanism.

Order matters at that seam. A queued message is the person speaking, so it wins: if
someone typed while the agent worked, the next turn is theirs, and the goal continues
after that.

## What judges "done"

Two things already answer this and neither should be duplicated:

- `agentdesk::auditor` reads the real diff after a run and returns `Passed`, `Hollow`,
  `Blocked` or `Unavailable`. `Hollow` is precisely "looks finished and is not", which
  is the case continuation exists for.
- `agentdesk::completion` judges a stricter condition (a named check passing, particular
  files changed) from recorded evidence.

A goal is met when the auditor passes and any attached condition is met. An
`Unavailable` audit does **not** count as met: an audit that could not run knows nothing,
and treating silence as success is the exact failure mode the auditor was built to close.
It ends the goal as unjudged instead, because continuing on no information would spend
someone's budget guessing.

## What a continuation turn is told

The next turn is given what the last one produced, not a fresh start: the auditor's
reasons when the verdict was hollow, the condition that was not met, and the same
transcript continuity every turn already gets. This is why continuation lives after the
audit rather than replacing it.

## Stopping

Stop must end the goal, not the turn. A Stop that only ended one turn and let the next
begin would be the worst possible behaviour here: the person would press it repeatedly
while the machine kept starting work. The cancel path already reaches the running turn;
the goal must be cleared in the same action, before the completion routing runs.

Three other things end a goal with no further turn: a declined gate (the person said no
to this specific thing, and asking again is not a continuation, it is nagging), a run
that ended failed or refused, and either budget ceiling.

## Budgets

`JobBudget` already carries `max_turns` and `max_seconds` and is enforced inside
`cli_run::run_task` per turn. A goal needs the same two limits across turns, which is a
separate counter: a ten-turn goal of twenty-turn turns is a different ceiling from
either alone. Reaching one ends the goal reported as out of budget, never as met.

## What this deliberately does not do

- No unattended authority. Every gate still asks. A goal makes an agent persistent, not
  autonomous, and a person who walks away from a gated run returns to a waiting question
  rather than to work that proceeded without them.
- No new "keep going" tool the model can call. The decision is the app's, from evidence,
  because a model asking to continue is the claim continuation is meant to check.
- No goal on a read-only chat that could turn into writing. The intent is fixed for the
  life of the goal.

## Open questions for implementation

- Whether a goal should survive a restart. The session is durable and the recovery sweep
  now marks interrupted runs, so a goal could resume; but resuming work unattended after
  a crash is a different promise from continuing while someone is present. Default to
  ending the goal on restart and revisit with real use.
- Whether the person should be able to set a goal on a manual chat, or only on a
  source-bound one. A task or issue states what done means; a manual chat's goal is
  whatever the person types, which is harder to judge.
