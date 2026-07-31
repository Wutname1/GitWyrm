# Tasks

> Skeleton - to be fleshed out. Open questions are listed in `proposal.md`.

## 1. Backend

- [ ] 1.1 `commands/worktree.rs`: list, add, remove, prune, repair. Prefer git2's
      worktree API; fall back to shell git where git2 lacks coverage, matching the
      existing local-vs-network split.
- [ ] 1.2 Broken-worktree detection (folder missing or moved) surfaced as a typed
      outcome, not an error string, per the repo's typed-outcome convention.
- [ ] 1.3 Branch-checked-out-elsewhere detection, returning which worktree holds it
      so the UI can offer to open that one.
- [ ] 1.4 File watcher covers open worktrees so external edits invalidate queries
      the same way they do for the main checkout.

## 2. UI

- [ ] 2.1 `WorktreesSection` in the left panel, modeled on `SubmodulesSection`.
- [ ] 2.2 Add-worktree modal: branch picker (existing or new), folder picker,
      default location outside the repository.
- [ ] 2.3 Open a worktree (decide: own tab vs. switchable checkout - see proposal).
- [ ] 2.4 Status bar names the active worktree.
- [ ] 2.5 Remove confirm stating uncommitted-change count; no type-to-confirm.
- [ ] 2.6 Broken worktree state with repair/prune action.

## 3. Spec Desk integration

- [ ] 3.1 "Run in its own worktree" option on run start.
- [ ] 3.2 Run console shows which worktree the run is working in.
- [ ] 3.3 Completed run reviewed as a diff before it reaches the user's branch.
- [ ] 3.4 Discard deletes the worktree; decide handling when the user hand-edited
      files in it (see proposal).

## 4. Verify

- [ ] 4.1 Add, open, commit in, and remove a worktree; main checkout unaffected
- [ ] 4.2 Attempt to check out a branch held by another worktree - plain explanation
- [ ] 4.3 Delete a worktree folder in Explorer - shows broken, prunes cleanly
- [ ] 4.4 Run a task in its own worktree while editing files in the main checkout
- [ ] 4.5 Discard an isolated run - folder gone, branch untouched
