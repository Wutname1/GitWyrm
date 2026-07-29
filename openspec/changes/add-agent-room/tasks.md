# Tasks

## 1. Spec phase (do these before any build tasks are added)

- [ ] 1.1 Decide isolation: worktree per agent vs sequential commits on one branch
- [ ] 1.2 Decide which tasks may run in parallel (independent files? explicit marking?)
- [ ] 1.3 Write the requirements as spec deltas (per-run cards, shared gate queue,
      per-run stop, room-level stop-all)
- [ ] 1.4 Design the commit story: one commit per task preserved, ordering rules,
      conflict handling when two agents touch one file
- [ ] 1.5 Validate cost/limits messaging for parallel runs against the plain-language rule
