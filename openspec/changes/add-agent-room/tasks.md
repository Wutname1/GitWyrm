# Tasks

## 1. Spec phase (do these before any build tasks are added)

- [x] 1.1 Decide isolation: worktree per agent vs sequential commits on one branch.
      **Decided: one worktree per agent.** Sequential commits on one branch means one
      working directory, so two agents interleave writes into the same files and
      afterwards nobody can tell whose change was whose. Worktrees cost disk and a
      little setup time, but git already understands them, so nothing custom has to be
      invented or trusted. No agent works in the directory the user has open.
- [x] 1.2 Decide which tasks may run in parallel (independent files? explicit marking?).
      **Decided: the user selects, and the room says what can go wrong.** Task text
      does not say what a task will touch, so inferring independence from wording means
      guessing, and a wrong guess produces exactly the conflict the room exists to
      avoid. Static file analysis was rejected too: it would have to predict what an
      agent is about to write, which is not knowable before the run. Selecting one task
      runs it as an ordinary single run rather than opening a room.
- [x] 1.3 Write the requirements as spec deltas (per-run cards, shared gate queue,
      per-run stop, room-level stop-all). Written in `specs/ai-runs/spec.md`.
- [x] 1.4 Design the commit story: one commit per task preserved, ordering rules,
      conflict handling when two agents touch one file. **Decided: one commit per task,
      applied in finish order, each carrying its task's `Spec:` trailer.** Finish order
      rather than start order, because start order would make an early agent block a
      finished one for no reason. A run whose work will not apply cleanly ends in a
      conflict the user resolves; the other runs carry on and nothing is committed for
      the conflicted one until they do.
- [x] 1.5 Validate cost/limits messaging for parallel runs against the plain-language
      rule. **Decided: state the agent count and that each uses the AI plan separately,
      before anything starts.** Parallel runs multiply spend in a way a single run does
      not, and learning that afterwards is the kind of surprise that loses trust for
      good.

## 2. Build (not yet scheduled)

The requirements above are written, but this change stays a draft until its
dependencies are proven in real use: `add-ai-task-runs`, `add-ai-agent-engine`, and
`add-ai-run-completion`. Building a room on top of a single-run engine that has not
been exercised would multiply any flaw in it by the number of agents.

One thing the build will have to change: `SessionRegistry` currently holds
`HashMap<repo_id, RunSession>` - one run per repository, with a second start refused
rather than replaced (`StartRefusal::AlreadyRunning`). The room needs many sessions per
repository, so that key becomes the session id and the refusal rule moves to "not the
same task twice". The frontend store is keyed the same way (`byRepo`) and follows.

That refusal is load-bearing today: it is what stops a double-click, or a forgotten run
in another window, from discarding work in progress. Whatever replaces it has to keep
that property.
