# Change: Worktrees (parallel checkouts, and isolation for AI runs)

## Why

A worktree is a second working folder for the same repository, checked out to a
different branch. Today the only way to work on two branches at once is to stash,
switch, and switch back - or clone the repository twice.

Two things make this worth building now:

1. **Interrupted work is the common case.** A review request arrives while the
   working tree is half-finished. Stash-switch-switch-back is the step most likely
   to lose someone's work, and it is exactly the ceremony GitWyrm exists to remove.
2. **Spec Desk runs need somewhere safe to work.** A run currently edits the same
   working tree the user is looking at. Running a task in its own worktree means the
   user keeps working while a task runs, and an unwanted result is thrown away by
   deleting a folder rather than by unpicking edits from live work.

Competing clients ship worktrees and market them as the isolation primitive for AI
sessions. GitWyrm pitches watched, one-task-at-a-time runs without yet offering the
isolation that makes that promise concrete.

Groundwork already shipped: the `enableWorktrees` setting, its General Settings toggle,
the `worktrees` sidebar section key, and the `has_worktrees` command that turns the
feature on by itself for a repository that already uses worktrees. This change fills in
everything that flag currently reveals nothing of.

## What Changes

- **Worktrees section in the left panel**, alongside Branches and Submodules: the
  worktrees that exist, which branch each is on, and which one is open
- **A worktree that exists is never hidden.** The `enableWorktrees` flag changes meaning:
  it governs whether GitWyrm offers to *create* worktrees, not whether it admits the ones
  already there. With the flag off and a worktree present - because a terminal or another
  client made one - the section appears and the worktree can be opened, removed, pruned,
  or repaired; only Add is withheld. With the flag off and none present, the section is
  absent, so a user who has never wanted this feature still never sees it.
- **Externally-made worktrees show up on their own**, detected by the file watcher rather
  than by the current once-per-repo-activation check, which cannot see a `git worktree
  add` typed in a terminal beside an already-focused window
- **Add a worktree**: pick a branch (or make one), pick a folder, defaulting to a
  sibling folder next to the repository so it is never accidentally committed
- **Open a worktree as its own repository tab** (see `design.md` - a linked worktree is a
  real working folder, so the existing open-by-path machinery carries it); the status bar
  names the worktree so the active checkout is never ambiguous
- **Remove a worktree**, with a plain-language confirm that says what happens to
  uncommitted changes in it
- **Removal that will not go through is walked out, not refused.** The most-reported
  worktree trap is a chain of linked refusals: the branch will not delete because a
  worktree holds it, the worktree will not delete because it is dirty, and on Windows the
  folder will not delete because a process still has a handle in it. Each raw git message
  names a symptom and no cause. GitWyrm resolves each in place - the branch-delete prompt
  offers to remove the worktree, the removal confirm offers keep-or-discard with modified
  and untracked counted separately, and the lock failure says a program is using the
  folder and offers Try again. `--force` is never something the user has to know.
- **GitWyrm releases its own hold before removing**, so the app watching a folder is never
  the reason that folder will not delete
- **Copy the ignored files a new worktree needs.** A fresh worktree has only tracked
  files, so local environment files are missing and the checkout looks broken for a reason
  git never mentions - the most common "my worktree doesn't work" report. GitWyrm offers to
  carry them, while refusing to copy large generated dependency folders.
- **After removal, offer to delete the branch too** when it is merged - cleaning up in two
  places is the step people forget
- **Repair/prune** for worktrees whose folder was moved or deleted outside GitWyrm,
  including the moved-main-repository case where every worktree breaks at once
- **Run this task in its own worktree** (Spec Desk option, opt-in per run): the run works
  in a disposable checkout, the user keeps editing their own, and the result is reviewed
  as a diff before it reaches the branch the user is on. Discarding a run deletes the
  folder - unless the user hand-edited files in it, in which case discard says so and
  offers to keep it.
- Worktree-aware guards: a branch checked out in another worktree cannot be checked
  out twice, and GitWyrm SHALL explain that in plain words - naming the worktree that
  holds it and offering to open it - instead of surfacing the raw git error

## Impact

- Affected specs: `worktrees` (new capability); `ai-runs` (run location option)
- Affected code: new `commands/worktree.rs` and `git/worktree.rs`; `WorktreesSection` in
  `left-panel/` modeled on `SubmodulesSection`; `AddWorktreeModal`; branch checkout guard
  in `commands/branch.rs`; the auto-enable effect in `App.tsx` (its once-per-activation
  check stops being what makes worktrees visible); status bar; repository tab titles;
  Spec Desk run start and discard paths (`commands/airun.rs`, `ai/agent/session.rs`)
- Decisions previously open, now settled in `design.md`: worktrees open as their own tab;
  the default location is a sibling folder; run isolation is opt-in; a hand-edited run
  worktree is never silently deleted
- Depends on: nothing new. The run-isolation part depends on `add-ai-agent-engine`
  having landed the run's working-directory plumbing
