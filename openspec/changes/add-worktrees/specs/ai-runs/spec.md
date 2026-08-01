# ai-runs Spec Delta

## MODIFIED Requirements

### Requirement: Starting a run

With an AI configured, a run SHALL be startable from the rail's next-task card, from
any open task row's hover action, and from the main-window spec card (which opens the
Desk in the same motion). Only one run per repository may be active; a second start
attempt SHALL explain this and route to the run tab. Starting SHALL be blocked while a
finished run awaits commit review.

Starting a run SHALL offer working in its own worktree, off by default, described in
plain words as letting the user keep editing while the task runs. When the option is
off, the run works in the user's own checkout with their edits set aside, as before.
When it is on, a worktree is created for the run and the guardrail line names it.

#### Scenario: Start from a task row

- WHEN the user clicks Run with AI on task 7's row
- THEN the run targets task 7 specifically, not the next open task

#### Scenario: One at a time

- WHEN a run is active and the user starts another
- THEN nothing new starts and the message routes them to the active run

#### Scenario: Isolation is offered, not imposed

- WHEN the user starts a single run
- THEN the option to work in its own folder is present and unticked, and leaving it
  unticked creates no folder on disk

#### Scenario: Isolation cannot be silently skipped

- WHEN the isolation option is ticked but a worktree cannot be created
- THEN the run does not start in the user's checkout instead - it explains what failed

### Requirement: Guardrails are stated where the run happens

Every run SHALL state its guardrails in the console: works only on the linked branch (or
a new work branch when none is linked), one commit per task, never pushes, stoppable
anytime. The stated guardrails SHALL match actual behavior exactly - a run must never do
something its own header says it will not.

When a run works somewhere other than the user's own checkout, the guardrail line SHALL
name that folder. A run that edits files the user cannot see while its header implies
otherwise is the dishonesty this requirement exists to prevent.

#### Scenario: The promise is on screen

- WHEN a run is active
- THEN the console names the branch it is limited to and states that it never pushes

#### Scenario: Push is never offered

- WHEN any gate is presented
- THEN pushing is not among the choices - it is refused by the engine, not permitted by a
  click

#### Scenario: Working elsewhere is stated

- WHEN a run works in its own worktree
- THEN the guardrail line names that folder, and says the user's own checkout is not
  being edited

### Requirement: One worktree per agent

Each agent in the room SHALL work in its own git worktree on its own branch. Agents
SHALL NOT share a working directory, and no agent SHALL work in the directory the user
has open.

Two agents editing one working tree would interleave writes to the same files, and
afterwards neither the user nor the runs could tell whose change was whose. Worktrees
give each agent a real checkout at the cost of disk, and git already understands them,
so no custom isolation has to be invented or trusted.

Agent worktrees SHALL be created outside the repository working tree, in the same place
a hand-made worktree defaults to, and SHALL appear in the Worktrees section like any
other while they exist. A folder the user can see is a folder the user can rescue work
from; a hidden one is not.

#### Scenario: Isolation is real

- WHEN two agents both edit files while running
- THEN each edit lands only in that agent's worktree
- AND the user's own working directory is untouched by either

#### Scenario: Worktree cleanup

- WHEN a run ends, is stopped, or fails
- THEN its worktree is removed once its work has been merged or discarded
- AND a worktree is never removed while it holds the only copy of an agent's work

#### Scenario: The room's folders are not hidden

- WHEN agents are running in the room
- THEN their worktrees are listed in the Worktrees section, marked as belonging to a run

#### Scenario: Cleanup respects hand edits

- WHEN a run's worktree is due for removal but the user has edited files in it by hand
- THEN it is kept and listed as an ordinary worktree rather than deleted
