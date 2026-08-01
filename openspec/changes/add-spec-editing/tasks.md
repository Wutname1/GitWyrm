# Tasks

## 1. Read and write spec files

- [x] 1.1 `openspec_read_file(repo_id, change_id, file)` returning the raw body.
      Reads go through the same path resolution as writes so both agree on what
      counts as inside the change package. A missing file reads as empty, so a
      change with no design.md opens a blank editor rather than an error.
- [x] 1.2 `openspec_write_file(repo_id, change_id, file, body)`. Resolves the
      path, refuses anything outside `openspec/changes/<id>/`, and returns a
      plain-language error rather than an io error string. Canonicalize before
      comparing: a `..` segment must not escape by way of a symlink. Lives in
      `openspec/write.rs` beside the other mutations, and writes the body
      byte-for-byte with no reformatting.
- [x] 1.3 Rust tests for the refusal: `../`, an absolute path, an unknown
      change, and a symlink pointing out of the change directory are each
      refused; round-trip and byte-preservation are covered too. 23 pass.
      **The symlink test is `#[cfg(unix)]` and does not run on this Windows
      machine** — creating one needs elevation or developer mode. The
      canonicalize check it covers is platform-independent, but the test itself
      is unverified here and wants a CI run on Linux.

## 2. The editor

- [x] 2.1 Added CodeMirror 6 (`state`, `view`, `lang-markdown`, `commands`,
      `language`) plus an explicit `@lezer/highlight` (it was resolving only
      transitively). No worker config needed, as expected.
      **Bundle cost is larger than the estimate that justified the choice:
      1568 kB -> 2081 kB raw, 446 kB -> 624 kB gzipped, so +513 kB raw and
      +178 kB gzip.** The "roughly a tenth of Monaco" claim in the proposal
      holds, but "~150 KB" did not.
      **Resolved by lazy-loading**: `SpecEditor` is behind a dynamic import, so
      CodeMirror is a separate 504 kB chunk fetched on the first Edit click. The
      main bundle is 1572 kB / 447 kB gzip, level with the 1568 kB baseline, so
      people who never edit a spec file pay nothing.
- [x] 2.2 `SpecEditor` component: markdown mode, restrained highlighting, themed
      entirely from the app's `--gw-*` tokens so every theme is covered without
      a per-theme map. Built once per mount with handlers in a ref, so typing
      does not rebuild the editor and lose the cursor or undo history.
- [x] 2.3 Edit mode per tab: an Edit control on Proposal, each delta card, Tasks
      and Design; Save and Cancel while editing; tabs render read-only until
      asked. Design is included because a change with no design.md could not
      otherwise get one from the Desk.
- [x] 2.4 Draft state per file in `stores/specDraftStore.ts`, surviving tab and
      change switches. Unsaved markers on the file and on the change breadcrumb.
- [x] 2.5 Confirm before discarding, naming the file, via the existing
      `ConfirmDialog` (no type-to-confirm, per the app's rule).
- [x] 2.6 Save writes, invalidates the spec queries, and closes back to the
      rendered view. A failed save keeps the draft and toasts the reason -- the
      draft is only dropped after the write returns.

## 3. AI-drafted edits

- [x] 3.1 `openspec_draft_edit(repo_id, change_id, file, instruction, provider,
      model)` returning the proposed full body, in `openspec/edit_draft.rs`. Its
      own prompt, its own result type, no disk write; `openspec_ask` untouched.
      Asks for the whole file rather than a patch -- a model inventing line
      numbers produces a diff that almost applies, which is worse than none, and
      GitWyrm computes the real difference itself. Parser tolerates a missing
      SUMMARY line and a wrapping code fence, refuses an empty body, and keeps
      fences and `---` rules that belong to the file. 11 tests.
- [x] 3.2 Ask recognises an edit request and offers to draft it, naming one file.
      Detection lives in `lib/specEditRequest.ts` so it can be tested (9 tests):
      it needs both an edit verb and a named file, so "what does tasks.md say?"
      stays a question and "reword this" does not guess among four files.
      Recognises the proposal by its section names ("the Why") and a delta by its
      capability folder, which is how people actually refer to them.
- [x] 3.3 A line diff in TypeScript (`src/lib/textDiff.ts`), comparing the draft
      against the file on disk. No Rust command: `commands/diff.rs` is
      git-object based and does not compare a working file to an in-memory
      string. Emits the `{ sign, text }` shape the diff view already renders, so
      `computeWordSpans` in the existing `wordDiff.ts` supplies intra-line
      highlighting — the `similar` fallback would have duplicated code already
      in the app. LCS over lines with common head/tail trimmed and a cell cap
      so a pathological input degrades instead of freezing.
      **Checked by a throwaway script, not a committed test: this repo has no
      JS test runner** (`npm test` is typecheck + cargo + `tauri dev`). 17
      checks pass, including the property that every line of both inputs
      appears in order. Adding vitest is its own decision, noted in 4.3b.
- [x] 3.4 Review UI (`DraftedEditReview.tsx`): the diff, with Reject and "Open in
      the editor". Accept puts the draft in the editor as unsaved text and does
      not write; the user saves. An AI edit therefore reaches disk by the same
      path and the same refusal as a hand edit, with no second write route to
      keep in step. The file is re-read at draft time so the diff compares
      against what is on disk now, not against what the AI was shown.

## 4. Verify

- [x] 4.1 Edit and save each file type (proposal, tasks, design, a delta); the
      Desk shows the saved text and the change list updates
- [ ] 4.2 Unsaved edits survive a tab switch and a change switch, and discarding
      asks first
- [ ] 4.3 Ask for an edit: a diff appears, Accept opens it unsaved in the editor,
      disk is unchanged until the user saves, and Reject leaves it untouched
- [ ] 4.3b The diff reads correctly on a real drafted edit (a few reworded
      sentences), not just on test fixtures — this is the check that decides
      whether the TS diff stands or the `similar` fallback is needed
- [ ] 4.4 A drafted edit naming a file outside the change package is refused
- [ ] 4.5 The editor reads correctly in both light and dark themes
