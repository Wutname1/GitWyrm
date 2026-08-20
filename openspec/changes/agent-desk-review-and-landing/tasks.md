# Tasks

## 1. Result model

- [ ] 1.1 Define result references for execution/helper, worktree, base/head, changed paths,
      checks, commit, source, OpenSpec task, and cleanup state.
- [ ] 1.2 Build result records from existing completion state; do not duplicate diff text.
- [ ] 1.3 Persist partial results for stopped/failed/conflicted executions.
- [ ] 1.4 Add typed states: reviewing, revision-requested, kept, committed, discarded,
      cleanup-needed, and cleanup-failed.

## 2. Review UI

- [ ] 2.1 Add changed-file list and combined summary linked to existing diff view.
- [ ] 2.2 Let graph node Output/View diff open its helper-scoped result.
- [ ] 2.3 Preserve transcript/graph selection when returning from diff review.
- [ ] 2.4 Show check outcomes and command names without raw terminal flood.
- [ ] 2.5 Add Review requested changes as a new lead message/execution step.

## 3. Keep, undo, and commit

- [ ] 3.1 Route Keep/Undo through existing run completion commands/outcomes.
- [ ] 3.2 Refuse Undo when hand edits would be destroyed; explain and preserve them.
- [ ] 3.3 Create intentional commit with user-facing subject and source/OpenSpec trailers.
- [ ] 3.4 Never auto-commit merely because the lead says finished.
- [ ] 3.5 Refresh graph/status/diff/source progress after keep/undo/commit.

## 4. Host handoff

- [ ] 4.1 Add Create pull request/Update pull request as a separate result action.
- [ ] 4.2 Reuse existing host capability/sign-in checks and branch metadata.
- [ ] 4.3 Require the existing explicit push path; the agent engine never pushes.
- [ ] 4.4 Draft PR title/body from source/result but make them editable before host action.
- [ ] 4.5 Never post review comments or issue updates implicitly.

## 5. Cleanup and recovery

- [ ] 5.1 Remove agent worktrees only after safe integration or confirmed discard.
- [ ] 5.2 Detect hand edits and keep the worktree with an Open action.
- [ ] 5.3 Reconcile orphaned markers/worktrees at startup without deleting automatically.
- [ ] 5.4 Archive sessions without deleting result provenance or external imports.
- [ ] 5.5 Add cleanup retry for Windows file-lock failures after releasing GitWyrm handles.

## 6. Hardening and migration cleanup

- [ ] 6.1 Complete `docs/agent-desk/acceptance-checklist.md` with named evidence.
- [ ] 6.2 Verify screen reader names, keyboard focus, reduced motion, and display scaling.
- [ ] 6.3 Profile 1,000 sessions, large transcript, and active three-helper graph.
- [ ] 6.4 Verify offline/reconnect/restart/cancel/conflict native scenarios.
- [ ] 6.5 Remove obsolete Spec Desk-only shell code in a separate mechanical commit.
- [ ] 6.6 Keep old URL/settings migration for at least one release unless product decides
      otherwise with explicit migration evidence.
- [ ] 6.7 Run typecheck, relevant Rust tests, binding export, and native build verification.
- [ ] 6.8 Record Gate 8 evidence.
