# Tasks

## 1. Sources and compatibility

- [ ] 1.1 Add OpenSpec change and exact-task source variants with cached context.
- [ ] 1.2 Build source identity from repo/change/task index, not task display number alone.
- [ ] 1.3 Route current Spec Desk selected change into Agent Desk source/detail.
- [ ] 1.4 Map current active run into a durable execution on the matching session.
- [ ] 1.5 Preserve old deep links for change ID and selected tab where possible.

## 2. Context builder

- [ ] 2.1 Load proposal, design, every delta, tasks, progress, branch link, and history.
- [ ] 2.2 Record honest absence for optional documents.
- [ ] 2.3 Include exact target task even when it is not the next open task.
- [ ] 2.4 Rebuild context on file-watcher refresh and mark launch-vs-live differences.
- [ ] 2.5 Render all repository markdown inertly.

## 3. Plan integration

- [ ] 3.1 Define proposed graph nodes with requirement/scenario/task references.
- [ ] 3.2 Persist plan draft as an execution record in AwaitingStart state.
- [ ] 3.3 Detect task/spec changes after draft and block Start until refreshed/accepted.
- [ ] 3.4 Add Revise plan, Start, and Use solo actions with immediate visible state.

## 4. File-backed completion

- [ ] 4.1 Route accepted task completion through existing task-line writer.
- [ ] 4.2 Route accepted spec edits through existing draft/review writer.
- [ ] 4.3 Refresh all main/Desk progress surfaces after writes.
- [ ] 4.4 Never tick a task solely because an execution emitted Finished; require existing
      review/completion policy.
- [ ] 4.5 Handle archived/deleted/moved changes without losing session history.

## 5. No-AI continuity and proof

- [ ] 5.1 Keep copy handoff, editor, opencode, and manual editing actions available.
- [ ] 5.2 Test repo without OpenSpec and repo without CLI.
- [ ] 5.3 Test task-number gaps/duplicates and starting a non-next task.
- [ ] 5.4 Native-test exact-task restart and file-watcher refresh.
- [ ] 5.5 Record Gate 4 evidence.
