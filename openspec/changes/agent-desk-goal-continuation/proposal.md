# Change: Keep working toward a goal until it is met or the budget runs out

## Why

Today a run does one turn's worth of work and stops. Anything longer is the person
noticing it stopped, reading what happened, and sending another message. That is the
whole loop for work that takes ten turns, and it means an agent cannot be left alone
even when every individual step is safe and reviewed.

The pieces that make continuing safe already exist and are used: a spec or task states
what done means, an independent auditor reads the real diff after a run and can send
hollow work back, helpers carry completion conditions that are now enforced, budgets
cap turns and seconds, and every write still passes the approval gate. What is missing
is the thing that decides to take another turn.

This is the post-v1 half of "Auto": not more authority per turn, but more turns under
the authority already granted.

## What Changes

- Add a durable goal to a session: what finishing means, and how it is judged.
- After a turn ends cleanly, decide whether the goal is met; if not, start the next
  turn automatically with what was learned, until the goal is met or a budget is spent.
- Make every continuation visible and stoppable: the chat says why it continued, Stop
  ends the whole goal rather than one turn, and the budget is shown while it runs.
- Never continue past a refusal, a failed audit that stayed hollow, or a gate the person
  declined. A person's "no" ends the goal.
- Record what each continuation cost, so the total is visible before it is large.

## Impact

- Extends `src-tauri/src/agentdesk/` (a goal on the session header, judged after each
  turn) and `src-tauri/src/commands/airun.rs`'s completion routing, which already knows
  how to start a follow-up turn for a queued message.
- Reuses `agentdesk::auditor` for judging and `agentdesk::completion` for conditions
  rather than adding a second notion of "done".
- Adds a goal banner and a Stop control to the Agent Desk conversation.
- No new authority: a continuation runs under the same policy, gates and worktree
  isolation as the turn before it.
