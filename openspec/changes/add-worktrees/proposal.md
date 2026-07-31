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

## What Changes

- **Worktrees section in the left panel**, alongside Branches and Submodules: the
  worktrees that exist, which branch each is on, and which one is open
- **Add a worktree**: pick a branch (or make one), pick a folder, with a suggested
  default location outside the repository so it is never accidentally committed
- **Open a worktree** as a repository tab; the status bar names the worktree so the
  active checkout is never ambiguous
- **Remove a worktree**, with a plain-language confirm that says what happens to
  uncommitted changes in it
- **Repair/prune** for worktrees whose folder was moved or deleted outside GitWyrm
- **Run this task in its own worktree** (Spec Desk option): the run works in a
  disposable checkout, and the result is reviewed as a diff before it reaches the
  branch the user is on. Discarding a run deletes the folder.
- Worktree-aware guards: a branch checked out in another worktree cannot be checked
  out twice, and GitWyrm SHALL explain that in plain words instead of surfacing the
  raw git error

## Impact

- Affected specs: `worktrees` (new capability); `ai-runs` (run location option)
- Affected code: new `commands/worktree.rs`; `LeftPanel` section alongside
  `SubmodulesSection`; repository tab + status bar (active worktree); Spec Desk run
  start path and discard path
- Open questions to settle while fleshing this out:
  - Does a worktree open as its own tab, or as a switchable checkout inside the
    existing repository tab?
  - Default location for new worktrees (sibling folder next to the repository?)
  - Is per-task worktree isolation the default for runs, or opt-in?
  - What happens to a worktree when the run that created it is discarded but the
    user has since edited files in it by hand?
