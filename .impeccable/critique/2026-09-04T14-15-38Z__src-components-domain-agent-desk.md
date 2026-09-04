---
target: Agent Desk conversation workspace UI
total_score: 26
max_score: 40
na_heuristics: 
p0_count: 1
p1_count: 4
target_identity: "file:C:\\code\\GitWyrm\\.claude\\worktrees\\agent-desk-openchamber-audit-0f40fb\\src\\components\\domain\\agent-desk"
timestamp: 2026-09-04T14-15-38Z
slug: src-components-domain-agent-desk
---
## Pass 28 -- 2026-09-04: design critique (dual-agent), no code changes

Method: two isolated sub-agents (design review; detector + mechanical sweeps),
synthesised and then re-verified in the parent. Every finding below was read in
the file before being recorded. Detector: 3 warnings, all three verified false
positives (two `role="tab"` mint underlines that *implement* the Selected Must
Read rule, one inline thought-block quote bar). Score 26/40.

### C1 (P0) -- Agent Setup crashes the window on the providers tab

`AgentCatalog.tsx` calls `useQuery` (:27) and `useState` (:31), returns early at
:49 (`isLoading`) and :57 (`isError`), then calls `useWorkspaceStore` twice at
:78-79. Hook count goes 2 -> 4 the moment the query settles, so React throws
"Rendered more hooks than during the previous render" on the loading->loaded
transition -- i.e. every normal open of the tab. The only `ErrorBoundary` is at
`App.tsx:587`, wrapping the whole app, so the entire window is replaced by the
crash-report screen.

Introduced by `94d2c0d` ("Choose which AI tool new chats start with"), which
appended the two hooks below pre-existing early returns. There is no ESLint in
this repo, so `react-hooks/rules-of-hooks` never ran; `tsc` cannot see it.

Swept every component in both target directories for the same shape (hook call
below a component-level early return, per component rather than per file):
**this is the only instance.** The first sweep missed it because the regex
assumed 2-space `return` indentation and these are 4-space inside an `if` --
recording that, since a guard that misses its own instance proves nothing.

### C2 (P1) -- The transcript byline prints a raw provider id

`ConversationPane.tsx:301` (and the identical `:366`) renders
`{message.provider}` bare. `message.provider` has exactly one producer:
`commands/agent_import.rs:307` `provider: Some(adapter_id.to_string())`; native
runs set `None` (`bridge.rs:663`). So an imported chat bylines every message
`vscode-copilot` while `ImportedBadge` two spans away resolves the same string
to "VS Code Copilot Chat" via `adapterDisplayName` -- which is already imported
into this file at :18. Twelfth instance of the raw-identifier class (F62, F96,
F111, F139, F15).

### C3 (P1) -- The agent avatar is a two-character slice of a fallback constant

`ConversationPane.tsx:52-56`: `const source = message.provider ?? 'Lead'; return
source.slice(0, 2)`. Native messages therefore avatar as "LE" (uppercased), and
imported ones as "VS"/"OP"/"CL". Not identity and not state, on the most-repeated
glyph in the central surface. Contrast `AgentGraphPanel.tsx:68`, which routes the
analogous token through `helperRoleLabel`.

### C4 (P1) -- Four hand-written copies of one run-state predicate, one already diverged

`failed || missingSource || interrupted` is written out at
`AgentGraphPanel.tsx:188`, `SessionRow.tsx:78` (different order) and
`agentGraphProjection.ts:196`. `ConversationPane.tsx:759` is a fourth copy that
**omits `missingSource`**. Meanwhile `agentDeskResult.ts:567` exports
`runIsActive` -- the inverse predicate -- from a module with 30+ exported,
unit-tested helpers, three lines from where two of the copies are used. Zero
tests reference `stoppedBadly`. This is qa-log #39 ("agreement that is not
enforced is a coincidence with a shelf life") on run state.

### C5 (P1) -- "View diff" on the result panel can do nothing at all

`ResultReviewPanel.tsx:445-452`: `handleOpenDiff` skips `unwrap`, inspects only
`res.status === 'ok' && res.data.kind === 'mainWindowNotOpen'`, and has no
`.catch()`. `agent_result_open_diff` returns `Result<_, AppError>`, so a backend
failure arrives as `status: 'error'` -- a branch that is never read and never
caught. Rule #1, on the review screen. The sibling `AgentGraphPanel.viewChanges`
(:148-162) does this correctly with `unwrap` in `try/catch` and a toast on all
three branches. Only unguarded promise chain in either directory.

### C6 (P2) -- A manufactured Error puts a stack trace in a toast

`SessionComposer.tsx:166` `throw new Error(outcome.kind)`. `UpdateSessionOutcome`
has four failure variants (`NotFound`, `Damaged { reason }`, `WriteFailed
{ detail }`, `Unavailable { detail }`) and every explanatory string is discarded.
The `.catch` at :169 passes it to `describeError`, which returns `e.stack` for an
`Error` (`log.ts:31`) -- so the toast description is a JavaScript stack trace.
Only site of this pattern in the target.

### C7 (P2) -- "Put it back" gives no response on the row clicked

`RecentConfigChanges.tsx:93-96` shares one mutation object across every row;
`disabled={undo.isPending}` greys out the whole list at once and the label is the
static string `Put it back`. The sibling screen does it correctly:
`AgentCopiesOnDisk.tsx:24` holds `clearing: string | null` and :154-157 renders a
spinner and "Clearing..." on the matched row only. Distinct from F99, which fixed
post-completion invalidation; this is the in-flight gap, on the screen that
reverses writes to other applications' config files.

### C8 (P2) -- "sessions" on screen where the product says "chats"

`ImportPicker.tsx:48, 130, 134, 148` say "sessions"; :143 in the same component
says "Looking for chats...". `agentImportDisplay.ts` uses "chats" throughout, and
F111 already renamed imported chats away from the wire word.

### Rejected after verification this pass

- All 3 detector warnings (see Method).
- `MessageActions` copy and `AgentGraphPanel.viewChanges` as Rule #1 gaps -- both
  fully covered (`copied` state swap; toasts on all three branches).
- `SessionComposer.tsx:158` as an unhandled promise chain -- has a real `.catch`.
- `CopyPreviewDialog.tsx:184` passing `status.state` to `SyncStateBadge` -- the
  badge routes through `syncBadgeLabel`, so no enum variant reaches the screen.
- `SyncTable.tsx:24` empty state as absence-for-failure -- `AgentSetupView.tsx:114-116`
  handles `isError` before it mounts.
- `AwaitingStartCard.tsx:70` `outcome.started_helpers` -- `bindings.ts:8341`
  genuinely declares that snake_case field; code matches type.
- `ResultReviewPanel` `provider` prop as a rendered raw id -- only ever passed as
  an argument, never rendered.
- Hooks-after-early-return in `ConversationPane`, `SessionComposer`,
  `SessionSidebar` -- all three are returns inside `useMemo`/`useEffect`
  callbacks, not component-level.
- Hardcoded theme colours: zero hits; 68 `var(--gw-*)` uses. Theme Contract holds.
- Rule #3 (type-a-word-to-confirm): zero hits.

### Carried, unchanged

A6, A9, A10, A11, A12, B2/B3, B4, B8, B10, B11, G4, G6, G8-G11, N6, N7, and the
standing native-acceptance gate (F3). The micro floor stays deferred;
`text-[Npx]` returned zero occurrences in these two directories this pass.

### Pass 28 close

The pass recurring shape is **the correct implementation sitting beside the
defective one**: `adapterDisplayName` imported into the file that prints the raw
id (C2); `runIsActive` exported three lines from two hand-written copies of its
inverse (C4); `viewChanges` toasting every branch while its sibling catches
nothing (C5); the `AgentCopiesOnDisk` per-row spinner beside the
`RecentConfigChanges` shared one (C7). Five of eight findings are a house pattern
that exists, is tested, and was not reached for.

C1 is the exception and the lead: not a design defect but a hard crash on a
shipping tab, invisible to `tsc` and to `cargo test`, in a repo with no ESLint.
Pass 27 added `cargo test --no-run` to CI after finding the Rust half of exactly
this gap -- the frontend half is still open. One lint rule
(`react-hooks/rules-of-hooks`) would close C1 permanently, and a JSX rule
forbidding bare `{x.provider}`/`{x.kind}` in text position would close the class
behind C2.
