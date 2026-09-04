---
target: Agent Desk conversation workspace UI
total_score: 31
max_score: 40
na_heuristics: 
p0_count: 1
p1_count: 3
target_identity: "file:C:\\code\\GitWyrm\\.claude\\worktrees\\agent-desk-openchamber-audit-0f40fb\\src\\components\\domain\\agent-desk"
timestamp: 2026-09-04T14-45-13Z
slug: src-components-domain-agent-desk
---
# Design critique, pass 29 — Agent Desk + Agent Setup

Method: dual-agent (A: design review; B: detector + proven sweeps), every finding
re-verified in the parent by opening the file.

## Design health: 31/40

| # | Heuristic | Score | Key issue |
|---|---|---|---|
| 1 | Visibility of System Status | 3 | `AgentCatalog.refresh()` ends in silence on failure |
| 2 | Match System / Real World | 4 | Role tokens, paths, enums all get plain sentences |
| 3 | User Control and Freedom | 4 | Undo, archive/restore, per-helper Remove, zero type-to-confirm |
| 4 | Consistency and Standards | 3 | Three toggles use a selected style that equals their own hover |
| 5 | Error Prevention | 3 | Delete dialog's warning is silent while still loading |
| 6 | Recognition Rather Than Recall | 3 | Graph empty state quotes a label that is not a control |
| 7 | Flexibility and Efficiency | 3 | Composer queueing still keyboard-only (carried A9) |
| 8 | Aesthetic and Minimalist | 3 | `text-2xs` dominates; hierarchy rests on weight/colour alone |
| 9 | Error Recovery | 3 | Absence-vs-failure discipline is excellent; two gaps remain |
| 10 | Help and Documentation | 2 | No in-product help; explanation is inline microcopy only |
| **Total** | | **31/40** | Good |

## Findings

1. **P0 — Delete dialog is silent about a working copy while it is still reading.**
   `SessionSidebar.tsx:113-122`. `enabled: pendingDelete != null` so the query starts
   when the dialog opens; during the in-flight window `data` is undefined (-> `[]`) and
   `isError` false, so neither warning renders and Delete is live. Reads as a confident
   "no working copy" at the one moment the app does not know. Per F131 the folder becomes
   unreachable. `copiesOnDisk.isLoading` is read nowhere in the file. The absence guard
   checks only `.isError`, so this passes the test while still misleading.

2. **P1 — "Open it on the host" cannot fail visibly.** `ResultReviewPanel.tsx:868-870`
   `void commands.agentResultOpenPullRequestPage(url)` — no status read, no `.catch`.
   Command returns `Result<(), AppError>` (`agent_result.rs:1083`). The dialog closes
   unconditionally (`PullRequestDraftDialog.tsx:95`), discarding the drafted title/body.
   Same defect as pass 28's C5, 400 lines from that fix in the same file.

3. **P1 — Refresh on the AI-tools screen fails silently.** `AgentCatalog.tsx:41-53`
   `unwrap` throws; no `catch`; both call sites are `void refresh()`. The empty state
   tells people to install a tool then press Refresh, so this is the one recovery path
   on that screen.

4. **P1 — Three toggles mark "on" with the exact declarations of their own hover.**
   `AgentWorkspaceToolbar.tsx:91,167` and `PaneDetailPopover.tsx:89` use
   `border-border bg-panel3 text-foreground` for pressed and
   `hover:border-border hover:bg-panel3 hover:text-foreground` for hover.
   `--gw-panel3` and `--gw-border` are the same hex, and `themes.ts` sets them identical
   in Slate, Midnight and Paper dark. Split View and source-bar visibility read as
   unset while the pointer is away. House pattern (`border-primary`/`bg-soft`/
   `text-accent-text`) is used 41 times elsewhere, including `AgentDeskTitleBar.tsx:64`.

5. **P2 — F87's fix never shipped; its scaffolding did.** `BatchReviewDialog.tsx:11,66-68,91`.
   `type BuildStage` declares `'failed'`; `setStage('failed')` has never existed in any
   commit. `failures` is never incremented or read; `attempted` is never read. Commit
   f78a994 added 4 lines plus 37 lines of audit prose recording the fix as done. A total
   preview failure still renders the same "Nothing could be previewed" as an empty
   selection, and shows no Apply button and no retry.

6. **P2 — A fourth hand-written copy of `runStoppedBadly`.** `agentGraphProjection.ts:193-198`
   writes the three-state disjunction longhand in a file that already imports
   `runIsActive` from the module exporting it, and whose own comment (`:28-35`) explains
   why hand-copying this rule is unsafe. Pass 28's C4 named this line and converted the
   other three sites.

7. **P2 — Graph empty state names a control that does not exist.** `AgentGraphPanel.tsx:480`
   says `Choose "A lead agent, up to 3 helpers" below`. That string is a passive `<span>`
   (`SessionComposer.tsx:467`); the chooser says "Lead + helpers" / "A team". "below" is
   also wrong — the panel docks left, right or bottom.

8. **P2 — Only hardcoded palette colour in either directory.** `AgentCatalog.tsx:192`
   uses `border-amber-500/40 bg-amber-500/10 text-amber-600 dark:text-amber-300`.
   `--gw-amber` (#fbbf24) differs from Tailwind amber-500 (#f59e0b), so two yellows sit
   together; the sibling `SyncStateBadge.tsx:22` does it correctly with the token.

## Rejected after verification
- All 3 detector warnings (tab underlines implementing Selected Must Read; a quote bar).
- Shared `disabled` on undo rows — the cited exemplar does the same deliberately.
- Missing `onError` on config mutations — handled one layer up in `useAgentConfig`.
- `source.provider` in NewChatLanding/SessionContextPanel — CI provider name, not an adapter id.
- Unguarded `animate-spin` — pulse/skeleton guarded, spinners deliberately not.
- `resultNeedsReview`/`summarizeChangedPaths`/`describeOutcomeKind` as unwired — used internally.
- G4, A6-A22, B2-B11, N6/N7 — correctly carried, re-verified `openSplit` still leaves `activePane`.
