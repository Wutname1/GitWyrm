# Tasks

## 1. Graph model and validation

- [ ] 1.1 Add graph, node, dependency, job, budget, completion, and graph-state types.
- [ ] 1.2 Validate one lead, max three helpers, acyclic dependencies, stable unique IDs,
      allowed paths, and explicit completion conditions.
- [ ] 1.3 Add fixture tests for every invalid shape and schema migration.
- [ ] 1.4 Persist proposed/running/final graph in the session execution record.

## 2. Plan flow

- [ ] 2.1 Let the lead emit a proposed graph through a typed engine tool/output.
- [ ] 2.2 Render AwaitingStart graph with Start, Revise, and Use solo.
- [ ] 2.3 Start only after revalidating current source/policy/worktree capacity.
- [ ] 2.4 Make every action change graph state immediately and visibly.

## 3. Helper runtime

- [ ] 3.1 Create a unique execution ID, branch, and marked worktree for each helper.
- [ ] 3.2 Give helpers bounded prompt/context, path allowance, turn budget, and done check.
- [ ] 3.3 Schedule dependency-ready helpers with max concurrency three.
- [ ] 3.4 Persist every helper event before broadcasting.
- [ ] 3.5 Reject stale/duplicate helper events by execution/sequence.
- [ ] 3.6 Keep lead and peers responsive when one helper waits at a gate.

## 4. Controls and approvals

- [ ] 4.1 Add per-helper Stop that cancels only that execution.
- [ ] 4.2 Add labeled Stop all in Graph header; cancel lead/helpers promptly.
- [ ] 4.3 Preserve uncommitted recoverable work on stop/failure.
- [ ] 4.4 Key approval cards/answers to helper execution and gate ID.
- [ ] 4.5 Show all waiting approvals in one queue without answering peers.

## 5. Integration

- [ ] 5.1 Queue completed helper results in completion order.
- [ ] 5.2 Review/apply each result through existing completion/commit plumbing.
- [ ] 5.3 Detect conflicts as a typed state; preserve base/helper/integration copies.
- [ ] 5.4 Let conflict resolution resume only that node's integration.
- [ ] 5.5 Run lead combined review/check step before final session completion.

## 6. UI and recovery

- [ ] 6.1 Project graph nodes from backend records; no separate frontend graph truth.
- [ ] 6.2 Show node state, role/model, current action, dependency, files, and output link.
- [ ] 6.3 Reconstruct running/waiting/conflicted graph after window/app restart.
- [ ] 6.4 Recover orphaned worktrees and never delete the only copy of work.
- [ ] 6.5 Test two independent edits, stopped peer, simultaneous gates, conflict, crash.
- [ ] 6.6 Record Gate 5 evidence.
