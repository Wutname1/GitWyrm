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
- [ ] 2.4 Wire context into live execution, rebuild it on file-watcher refresh, and mark
      launch-vs-live differences.
      PARTIAL as of 2026-08-21: `start_execution_at` (`src-tauri/src/commands/agent_desk.rs`)
      now genuinely folds `resolve_openspec_context` into the live prompt and persists a
      `context_fingerprint` on the execution record, and `refresh_source_at`'s OpenSpec
      branch calls the identical `resolve_openspec_context`/`resolve_change_status` builder,
      so "live execution" and "refresh" share one code path (R5.1-R5.3 landed). Still
      missing: nothing on the frontend reads `context_fingerprint` or shows the user that
      the source has diverged since launch -- grepped `src/components/domain/agent-desk/*`
      and `src/lib/agentDesk*.ts`, zero hits. Left unchecked because the task bundles three
      things and the third (mark launch-vs-live differences) is not built.
- [ ] 2.5 Render all repository markdown inertly.

## 3. Plan integration

- [x] 3.1 Define proposed graph nodes with requirement/scenario/task references.
- [ ] 3.2 Persist plan draft as an execution record in AwaitingStart state.
- [ ] 3.3 Detect task/spec changes after draft and block Start until refreshed/accepted.
- [ ] 3.4 Add Revise plan, Start, and Use solo actions with immediate visible state.

## 4. File-backed completion

- [ ] 4.1 Route accepted task completion from the mounted review flow through the existing task-line writer.
      Confirmed orphan as of 2026-08-21: the backend command
      `agent_session_complete_openspec_task` (`src-tauri/src/commands/agent_desk.rs`) genuinely
      calls the shared `openspec::write::toggle_task_line` writer, and a frontend hook
      (`useCompleteOpenSpecTask` in `src/hooks/useOpenspecSessionSource.ts`) genuinely calls
      that command -- but grepping all of `src/**/*.tsx` for `useCompleteOpenSpecTask` finds
      zero imports. No mounted component ever calls it. The command and hook are real and
      correct in isolation; nothing in the rendered UI reaches them.
- [ ] 4.2 Route accepted spec edits through existing draft/review writer.
- [ ] 4.3 Refresh all main/Desk progress surfaces after writes from the production flow.
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

## Status 2026-08-21 (second pass)

Re-audited against this session's R5 landing claims. One finding: 2.4 is now genuinely
partial rather than fully missing -- `start_execution_at` folds OpenSpec context into the
live prompt and persists `context_fingerprint`, and `refresh_source_at` shares the same
context builder, confirming the "wire into live execution + rebuild on refresh" two-thirds
of the task. It stays unchecked because the third requirement (mark launch-vs-live
differences visibly) has no frontend consumer of `context_fingerprint` anywhere.

Also confirmed by direct import-grep (not just plausibility) that 4.1 is a genuine orphan:
the Rust command and the frontend hook that calls it are both correct, but no `.tsx`
component imports the hook. Left unchecked, as it already was, with the specific evidence
now recorded above rather than inferred.

No other changes from the 2026-08-20 pass's conclusions.
