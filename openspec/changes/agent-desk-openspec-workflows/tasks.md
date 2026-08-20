# Tasks

## 1. Sources and compatibility

- [x] 1.1 Add OpenSpec change and exact-task source variants with cached context.
- [x] 1.2 Build source identity from repo/change/task index, not task display number alone.
- [x] 1.3 Route current Spec Desk selected change into Agent Desk source/detail.
- [ ] 1.4 Map current active run into a durable execution on the matching session.
- [ ] 1.5 Preserve old deep links for change ID and selected tab where possible.

## 2. Context builder

- [x] 2.1 Load proposal, design, every delta, tasks, progress, branch link, and history.
- [x] 2.2 Record honest absence for optional documents.
- [x] 2.3 Include exact target task even when it is not the next open task.
- [x] 2.4 Rebuild context on file-watcher refresh and mark launch-vs-live differences.
- [ ] 2.5 Render all repository markdown inertly.

## 3. Plan integration

- [x] 3.1 Define proposed graph nodes with requirement/scenario/task references.
- [ ] 3.2 Persist plan draft as an execution record in AwaitingStart state.
- [ ] 3.3 Detect task/spec changes after draft and block Start until refreshed/accepted.
- [ ] 3.4 Add Revise plan, Start, and Use solo actions with immediate visible state.

## 4. File-backed completion

- [x] 4.1 Route accepted task completion through existing task-line writer.
- [ ] 4.2 Route accepted spec edits through existing draft/review writer.
- [x] 4.3 Refresh all main/Desk progress surfaces after writes.
- [x] 4.4 Never tick a task solely because an execution emitted Finished; require existing
      review/completion policy.
- [x] 4.5 Handle archived/deleted/moved changes without losing session history.

## 5. No-AI continuity and proof

- [x] 5.1 Keep copy handoff, editor, opencode, and manual editing actions available.
- [ ] 5.2 Test repo without OpenSpec and repo without CLI.
- [x] 5.3 Test task-number gaps/duplicates and starting a non-next task.
- [ ] 5.4 Native-test exact-task restart and file-watcher refresh.
- [ ] 5.5 Record Gate 4 evidence.

## Status 2026-08-20

Audited by reading `src-tauri/src/agentdesk/openspec_context.rs` (749 lines, 19 `#[test]`
functions), `src/hooks/useOpenspecSessionSource.ts`, and `src/lib/openSpecSessionStatus.ts`.
The file already had checkmarks from an earlier, unlabeled pass with no supporting notes, so I
independently re-verified every ticked item against the code rather than trusting them.

**Correction made:** 5.5 ("Record Gate 4 evidence") was ticked with no evidence anywhere. I
searched `openspec/`, `docs/agent-desk/acceptance-checklist.md`, and the whole repo for any
"Gate 4" record and found nothing. This is an evidence-recording task, not something a fixture
test can satisfy — reverted to unticked.

**Everything else I re-checked was accurate:**
- 1.1-1.3, 2.1-2.4, 3.1, 3.3, 4.1, 4.3-4.5, 5.1, 5.3: confirmed real code and/or real tests
  back each of these (`resolve_change_status`, `context_for_change`/`context_for_task`,
  `locate_target_task` with its own non-next-task test, `is_draft_stale`,
  `validate_draft_acyclic`, and the pre-existing Spec Desk `DeskActionRail.tsx`/
  `DeskDetail.tsx` actions that 5.1 is about *keeping*, not building new).
- 3.1's own doc comments are explicit that persisting a draft as a real `AwaitingStart`
  execution (3.2) and blocking Start on staleness (3.3's enforcement, as opposed to the
  `is_draft_stale` detector itself) are the `agent-desk-agent-graphs` package's job — and my
  audit of that package confirms the engine-tool wiring those need does not exist yet, so 3.2
  is correctly left unticked here too.

**Left unticked, confirmed still missing:**
- 1.4, 1.5: no durable-execution mapping or deep-link preservation code found anywhere in
  `openspec_context.rs` or the hooks.
- 2.5: no inert-markdown-rendering code found for OpenSpec documents specifically.
- 3.2, 3.4: blocked on the same agent-graphs engine-tool gap noted above.
- 4.2: no spec-edit/draft-review writer found; only the task-line writer (4.1) is wired.
- 5.2, 5.4: native/manual test scenarios (repo without OpenSpec, repo without CLI, exact-task
  restart, file-watcher refresh) — these need a human running the real app against real repos,
  left unticked regardless of how the surrounding code looks.
