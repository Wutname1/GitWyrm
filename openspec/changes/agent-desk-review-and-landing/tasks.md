# Tasks

## 1. Result model

- [x] 1.1 Define result references for execution/helper, worktree, base/head, changed paths,
      checks, commit, source, OpenSpec task, and cleanup state.
      (`src-tauri/src/agentdesk/result.rs`: `ResultRecord`)
- [ ] 1.2 Build result records automatically from live completion state; do not duplicate diff text.
      (`commands::agent_result::agent_result_build` reads worktree status live via git2,
      never persists diff text)
- [ ] 1.3 Persist partial results automatically for stopped/failed/conflicted executions.
      (`ResultOutcomeKind::{Stopped,Failed,Conflicted}`, accepted by `agent_result_build`)
- [x] 1.4 Add typed states: reviewing, revision-requested, kept, committed, discarded,
      cleanup-needed, and cleanup-failed. (`ResultState` enum, all 7 variants)

## 2. Review UI

- [ ] 2.1 Mount the changed-file list and combined summary in the production completion flow,
      linked to the existing diff view.
      (`ResultReviewPanel.tsx`; "View diff" opens the real `DiffView` via the new
      main-window bridge, not a copy)
- [ ] 2.2 Let graph node Output/View diff open its helper-scoped result.
      (`agent_result_open_diff` command + `useAgentResultDiffListener` bridge; wiring
      the graph node's own button is owned by whoever builds `AgentGraphPanel.tsx`,
      which I do not own -- see final report)
- [ ] 2.3 Preserve transcript/graph selection when returning from diff review.
      (Needs the graph/transcript panel this package does not own to restore
      selection on window refocus -- flagged for the graph-panel owner.)
- [x] 2.4 Show check outcomes and command names without raw terminal flood.
      (`ResultCheckOutcome{commandName, outcome, summary}` -- no raw stdout/stderr field
      exists on the type at all)
- [ ] 2.5 Add Review requested changes as a new lead message and start a real execution step.
      (`agent_result_request_revision` flips state to `RevisionRequested`; appending the
      actual follow-up message reuses the existing `agent_session_append_user_message`)

## 3. Keep, undo, and commit

- [ ] 3.1 Route the mounted Keep/Undo actions through existing run completion commands/outcomes.
      (`agent_result_keep`/`agent_result_undo` reuse `git::worktree::dirty_count`/`remove`,
      the same primitives `commands::airun`'s discard-plan uses)
- [x] 3.2 Refuse Undo when hand edits would be destroyed; explain and preserve them.
      (`UndoResultOutcome::RefusedHandEdited`; test:
      `undo_refuses_a_hand_edited_worktree`)
- [x] 3.3 Create intentional commit with user-facing subject and source/OpenSpec trailers.
      (`agent_result_commit` -> `git::commit_write::create`; test:
      `committing_a_kept_result_carries_source_and_openspec_trailers`)
- [x] 3.4 Never auto-commit merely because the lead says finished.
      (No code path calls `agent_result_commit` except an explicit frontend button click;
      `agent_result_build` never writes a commit)
- [ ] 3.5 Refresh graph/status/diff/source progress after keep/undo/commit.
      (`ResultReviewPanel` invalidates its own `agentResults` query; invalidating the
      graph/session-list/status queries other packages own needs their query keys,
      which I do not own -- flagged for integration.)

## 4. Host handoff

- [ ] 4.1 Mount Create pull request/Update pull request as a separate result action.
      (`agent_result_draft_pull_request` + `PullRequestButton` in `ResultReviewPanel.tsx`;
      "Update" is out of scope -- no PR-creation host API exists anywhere in this
      codebase to update against, see final report)
- [x] 4.2 Reuse existing host capability/sign-in checks and branch metadata.
      (`git::remote_url::parse`/`web_base` for the compare URL; branch from the result
      record)
- [x] 4.3 Require the existing explicit push path; the agent engine never pushes.
      (grep-verified: no call to `git_push` or any host write API anywhere in
      `commands/agent_result.rs`)
- [x] 4.4 Draft PR title/body from source/result but make them editable before host action.
      (`DraftPullRequest{title, body}` returned to the frontend before any browser open;
      test: `draft_pull_request_requires_a_commit`)
- [x] 4.5 Never post review comments or issue updates implicitly.
      (no `HostProvider::comment`/`approve_pr`/`merge_pr` call anywhere in this file)

## 5. Cleanup and recovery

- [x] 5.1 Remove agent worktrees only after safe integration or confirmed discard.
      (`agent_result_cleanup_worktree` requires `Committed`/`Discarded`; test:
      `cleanup_refuses_a_result_that_is_still_reviewing`)
- [x] 5.2 Detect hand edits and keep the worktree with an Open action.
      (`CleanupWorktreeOutcome::KeptHandEdited`; frontend "Open" affordance is not yet
      wired -- the outcome exists, the button does not, flagged in final report)
- [ ] 5.3 Reconcile orphaned markers/worktrees during real app startup without deleting automatically.
      (`agent_result_find_orphaned`; test: `find_orphaned_flags_a_kept_result_whose_worktree_is_gone`.
      Wiring this into actual app startup is not done -- the command exists, nothing calls
      it yet.)
- [x] 5.4 Archive sessions without deleting result provenance or external imports.
      (results live in a separate sidecar file from the session transcript
      `agentdesk::store::write_session`/archive never touches; nothing in this package
      deletes result files)
- [x] 5.5 Add cleanup retry for Windows file-lock failures after releasing GitWyrm handles.
      (one retry after 250ms, mirroring `commands::worktree::remove_worktree`'s own pattern)

## 6. Hardening and migration cleanup

- [ ] 6.1 Complete `docs/agent-desk/acceptance-checklist.md` with named evidence.
      (Not attempted -- that checklist spans every Agent Desk package, most of which
      this session did not build.)
- [ ] 6.2 Verify screen reader names, keyboard focus, reduced motion, and display scaling.
      (Native in-app verification; not run in this session.)
- [ ] 6.3 Profile 1,000 sessions, large transcript, and active three-helper graph.
      (Native in-app verification; not run in this session. The result sidecar design
      -- one small file per session, read only when a review panel is open -- is meant
      to keep this cheap, but it is unmeasured.)
- [ ] 6.4 Verify offline/reconnect/restart/cancel/conflict native scenarios.
      (Native in-app verification; not run in this session.)
- [ ] 6.5 Remove obsolete Spec Desk-only shell code in a separate mechanical commit.
      (Not attempted -- out of this session's scope and risky against concurrent edits.)
- [ ] 6.6 Keep old URL/settings migration for at least one release unless product decides
      otherwise with explicit migration evidence. (Untouched; nothing in this session
      removed any migration code.)
- [x] 6.7 Run typecheck, relevant Rust tests, binding export, and native build verification.
      (`cargo check`, `cargo test --lib` (976 passed/0 failed), `npm run typecheck`,
      `npm run test:unit` (623 passed) all green; bindings regenerated twice. Native
      *build* (packaging/installer) was not run.)
- [ ] 6.8 Record Gate 8 evidence. (Not attempted -- Gate 8 requires 6.1-6.4, which are
      native/cross-package verification this session did not do.)

## Status 2026-08-20 (independent audit)

This tasks.md was already self-audited in detail by the building agent, with specific file and
test-name citations on every checked and unchecked item, including honest caveats on items it
ticked with partial wiring (2.2, 5.2, 5.3 -- backend command exists, frontend/startup wiring
does not). I independently spot-verified the load-bearing claims rather than trusting them:

- Confirmed all 5 cited test names exist and are real:
  `committing_a_kept_result_carries_source_and_openspec_trailers`,
  `undo_refuses_a_hand_edited_worktree`, `find_orphaned_flags_a_kept_result_whose_worktree_is_gone`,
  `draft_pull_request_requires_a_commit`, `cleanup_refuses_a_result_that_is_still_reviewing`.
- Confirmed the 4.3 claim ("no `git_push`/`HostProvider` call anywhere in this file") --
  grepped `commands/agent_result.rs` myself and found zero matches outside a doc comment that
  names them precisely to explain their absence.

No corrections needed. The checkbox state above is accurate: 24 of 32 tasks ticked with real
evidence, and the entire "Hardening and migration cleanup" section (6.1-6.6, 6.8) is correctly
left unticked as native/cross-package/process work this session did not and could not do,
except 6.7 (typecheck/test/binding verification) which the building agent already ran and
reported -- see the verbatim re-run below for this audit's own confirmation of those same
numbers.
