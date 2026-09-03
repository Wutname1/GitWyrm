# Tasks: goal continuation

## 1. The goal itself

- [ ] 1.1 Add a durable goal to the session header: text, how it is judged, turn and time
      ceilings, and what has been spent. `#[serde(default)]` so older sessions load.
- [ ] 1.2 Set a goal when a session is created from a source that states one (an OpenSpec
      task's text), and let the person write or edit one between turns.
- [ ] 1.3 Refuse to edit a goal while a turn is running, and say why rather than
      silently ignoring the edit.
- [ ] 1.4 Show the goal, turns spent and budget remaining in the conversation while it is
      active.

## 2. Judging

- [ ] 2.1 Judge a finished turn against the goal using `agentdesk::auditor`'s verdict and
      any `agentdesk::completion` condition. Never the agent's own claim.
- [ ] 2.2 Treat an `Unavailable` audit as unjudged: end the goal and say the work could
      not be checked, rather than continuing or claiming success.
- [ ] 2.3 Unit tests: passed audit ends the goal met; hollow continues; unavailable ends
      unjudged; a met completion condition plus a passed audit is met.

## 3. Continuing

- [ ] 3.1 After a clean turn, start the next one when the goal is unmet, carrying the
      auditor's reasons or the unmet condition into the prompt.
- [ ] 3.2 A queued user message wins over a continuation: the person's turn runs first,
      and the goal continues after it.
- [ ] 3.3 A continuation runs under the same intent, mode, team, provider and worktree as
      the turn before it, with no widened authority.
- [ ] 3.4 Append a visible note saying the run continued and why, before the turn starts.
- [ ] 3.5 Unit tests: continuation reuses the previous turn's policy; a read-only session
      never gains write authority; a queued message takes precedence.

## 4. Stopping and bounds

- [ ] 4.1 Stop ends the goal, not just the turn, and says so.
- [ ] 4.2 A declined approval gate ends the goal with no further turn.
- [ ] 4.3 A failed, stopped or refused run ends the goal.
- [ ] 4.4 Enforce the goal's turn and time ceilings across turns, separate from any single
      turn's budget; reaching one ends the goal as out of budget, never as met.
- [ ] 4.5 Unit tests for each ending: stop, declined gate, failure, both ceilings.

## 5. Cost

- [ ] 5.1 Record each continuation's usage on its own execution, rolled into the session
      total the usage panel already shows.
- [ ] 5.2 Unit test: a goal across several turns reports the sum, with unreported figures
      staying absent rather than becoming zero.

## 6. Acceptance

- [ ] 6.1 Native: set a goal on a real OpenSpec task, watch it take more than one turn,
      and confirm the chat says why each turn started.
- [ ] 6.2 Native: press Stop mid-goal and confirm no further turn begins.
- [ ] 6.3 Native: decline a gate mid-goal and confirm the goal ends rather than asking
      again.
- [ ] 6.4 Native: let a goal hit its turn ceiling and confirm it reports out of budget
      rather than finished.
- [ ] 6.5 Record Gate evidence for this package.
