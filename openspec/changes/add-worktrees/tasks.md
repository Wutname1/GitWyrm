# Tasks

Decisions this plan assumes are settled in `design.md`: worktrees open as their own
repository tab; the default location is a sibling folder; single-run isolation is
opt-in while the agent room already requires it; a hand-edited run worktree is never
silently deleted.

## 1. Backend: the worktree model

- [x] 1.1 `git/worktree.rs`: a `Worktree` type carrying folder name, absolute path,
      branch, whether it is the main checkout, whether it is currently locked, and a
      typed `WorktreeState` (`ok` / `missing` / `moved`). Unit-testable against a
      temp repo, no Tauri types in this module - the same split as `git/submodule.rs`.
- [x] 1.2 `list`: git2 `Repository::worktrees` + `find_worktree`, with the main
      checkout included as the first entry so the list is the whole picture. Resolve
      each one's branch; a detached worktree reports its short sha instead.
- [x] 1.3 Broken detection as a typed outcome, not an error string: distinguish
      *missing* (folder gone -> prune) from *moved* (admin files point somewhere that
      is no longer a checkout -> repair). The UI offers one action, not both, so this
      distinction is load-bearing.
- [x] 1.4 `add`: shell `git worktree add`, with `-b` when creating a branch. Validates
      first - target folder empty or absent, branch not already checked out elsewhere -
      so failures happen before anything is written to disk.
- [x] 1.5 `remove`: refuses the currently open worktree and the main checkout; reports
      modified and untracked counts for the confirm before removing anything. Never
      deletes the branch. Takes an explicit discard flag that the UI only sets after its
      own plain-language confirm - `--force` is never surfaced as a user-facing concept.
- [x] 1.6 `prune` (git2 `worktree_prune`) and `repair` (shell `git worktree repair
      <path>`), each reachable only from the state that calls for it. `repair` also
      handles the moved-main-repository case, where every worktree breaks at once.
- [x] 1.7 `dirty_count(path)`: modified and untracked counts kept separate, for a
      worktree that is not the open one. Used by the remove confirm and by run-discard's
      hand-edit check. Separate because discarding untracked files is the one case with
      no way back.
- [x] 1.8 Typed removal outcomes: `removed`, `refused_dirty { modified, untracked }`,
      `refused_locked { path }` (a process holds a handle - the Windows case), and
      `partially_removed { path }`. Each carries what its offer needs; none is a string.
- [x] 1.9 Keep-the-changes path: set a dirty worktree's changes aside recoverably before
      removing it, reusing the existing stash plumbing rather than inventing a second
      way to park work.
- [x] 1.10 `branch_delete` gains a typed refusal naming the worktree that holds the
      branch, so the branch-delete prompt can offer to remove it.
- [x] 1.11 Ignored-file survey for a new worktree: list ignored files worth copying,
      excluding large generated dependency directories. Powers the copy offer at create
      time; the exclusion is what keeps it from proposing to copy a dependency tree.

## 2. Backend: commands and guards

- [x] 2.1 `commands/worktree.rs` exposing list / add / remove / prune / repair /
      dirty_count over the `git/worktree.rs` layer. Regenerate `bindings.ts` with the
      bindings script - never hand-edit it.
- [x] 2.2 Branch-checked-out-elsewhere detection returning *which* worktree holds the
      branch, so the UI can offer to open that one. Wired into the checkout path in
      `commands/branch.rs` and into add-worktree validation, not just one of them.
- [x] 2.3 Default-path suggestion: `../<repo-name>-<branch-slug>`, de-duplicated if
      that folder already exists. Computed in Rust so the modal and any future caller
      agree on it.
- [x] 2.4 File watcher covers open worktrees, and watches the repository's worktree
      admin directory so add/remove/move performed outside GitWyrm invalidates the list
      while the repository stays open and focused. This watch is what makes an
      externally-created worktree appear on its own; a re-check on repo activation
      cannot see a terminal running beside an already-focused window.
- [x] 2.5 Release before remove: stop watching the worktree path and drop any open
      repository handle for it before attempting removal, so GitWyrm is never itself the
      process blocking the delete. Retry once after release before reporting a lock.
- [x] 2.6 Unit tests over a temp repo: add, list (including main checkout), branch
      collision, remove refusals (dirty with each count shape, open worktree, main
      checkout), keep-changes path, missing vs. moved classification, prune, repair,
      branch-delete refusal naming the holder, ignored-file survey exclusions.

## 3. UI: the section

- [x] 3.1 `WorktreesSection` in `left-panel/`, modeled on `SubmodulesSection`: rows
      with folder name, branch, an open marker, and a per-row context menu.
- [x] 3.2 Three visibility states, not two: setting off and none exist -> no section;
      setting off and some exist -> section with open/remove/prune/repair but no Add,
      and a line saying creating worktrees is turned off in settings; setting on ->
      full section including Add and the empty state below.
- [x] 3.3 Empty state (setting on, none exist): a plain-language line saying what a
      worktree is, plus Add. The section is present before it is needed - that is how
      the feature gets found.
- [x] 3.4 Auto-enable is a first-encounter courtesy only: it fires when the user has
      never touched the setting, and never after a deliberate off. Replace the current
      `App.tsx` effect, whose once-per-activation check is no longer what makes
      worktrees visible.
- [x] 3.5 Broken rows: `missing` shows Prune, `moved` shows Repair, each with a
      one-line explanation of what happened. Repair asks where the folder went.
- [x] 3.6 `useWorktrees` query in `useGitQueries.ts` and mutations in
      `useGitMutations.ts`, invalidating on the watcher events from 2.4. The section's
      own presence is driven by this query, so an externally-created worktree makes it
      appear without any other trigger.
- [x] 3.7 An open worktree tab whose folder disappears externally reports that plainly
      rather than showing stale contents.

## 4. UI: add, open, remove

- [x] 4.1 `AddWorktreeModal`: branch picker (existing or new, saying which commit a
      new branch starts from), folder field prefilled from 2.3, native folder picker,
      and the full path stated before Create. Warns when the chosen folder is inside
      the repository working tree; refuses a non-empty folder with a plain message.
- [x] 4.2 Open a worktree as its own repository tab via the existing open-by-path
      path. If it is already open, switch to that tab rather than opening a second.
- [x] 4.3 Tab title and subtitle disambiguate sibling checkouts of one repository
      (folder name + branch), so two tabs of the same repo are never identical.
- [x] 4.4 Status bar names the active worktree when the open checkout is not the main
      one.
- [x] 4.5 Remove confirm via `ConfirmDialog`: states modified and untracked counts (or
      that there is nothing unsaved), calls out that untracked files have never been
      saved anywhere, states that the branch survives, offers Keep the changes / Discard
      and remove, and has no type-to-confirm.
- [x] 4.6 Branch-taken explanation surfaces wherever a checkout starts: names the
      worktree holding it and offers Open that worktree.
- [x] 4.7 Branch-delete prompt handles the held-by-worktree refusal: names the worktree,
      offers to remove it, and on success continues with the branch deletion the user
      originally asked for. This is the chain that makes branches feel undeletable.
- [x] 4.8 Locked-folder failure state: says a program is still using the folder, suggests
      closing editors or terminals open in it, and offers Try again. Never surfaces a raw
      permission error. `partially_removed` says what state the folder was left in.
- [x] 4.9 After a successful removal, offer to delete the branch when it is merged;
      say the work is still on it and offer nothing when it is not.
- [x] 4.10 Create-time copy offer in `AddWorktreeModal`: lists ignored files found worth
      copying with a checkbox, notes that dependency folders will need rebuilding, and is
      absent entirely when there is nothing to carry.

## 5. Spec Desk integration

- [x] 5.1 "Work in its own folder so you can keep editing" option on single-run start,
      unticked by default. Provisioning goes through the same path the agent room
      uses; the room never asks.
- [x] 5.2 Run working-directory plumbing: the run's tools operate in the worktree path
      rather than the repository path, and the guardrails' in-repo path check accepts
      that folder as the run's repository.
- [x] 5.3 Guardrail line names the folder whenever the run is not working in the user's
      checkout. If a worktree cannot be created, the run fails to start with an
      explanation - it never quietly falls back to the user's checkout.
- [x] 5.4 Run worktrees are listed in the Worktrees section while they exist, marked as
      belonging to a run.
- [x] 5.5 Completed isolated run is reviewed as a diff before anything reaches the
      user's branch.
- [x] 5.6 Discard: remove the worktree when it holds only what the run wrote; when
      `dirty_count` shows hand edits, say so and offer to keep the folder. A kept
      folder becomes an ordinary listed worktree.

## 6. Verify

Driven on screen in a native window - not devtools - per `dev-vs-prod-app-testing`.

- [ ] 6.1 Add a worktree from the modal; confirm the default path is a sibling folder
      and the created path matches what the dialog said
- [ ] 6.2 Open it, commit in it, and confirm the main checkout tab is unaffected and
      the status bar named the right checkout
- [ ] 6.3 Two tabs of the same repository are distinguishable at a glance
- [ ] 6.4 Attempt to check out a branch held by another worktree - plain explanation
      naming the worktree, and the Open action works
- [ ] 6.5 Add-worktree with a branch already checked out elsewhere - caught before
      anything is created
- [ ] 6.6 Delete a worktree folder in Explorer - shows as missing, prunes cleanly
- [ ] 6.7 Move a worktree folder in Explorer - shows as moved, repairs to the new path
- [ ] 6.7a Move the whole repository folder - worktrees break together and are offered a
      single repair, not a list of separate broken rows
- [ ] 6.7b Create a worktree in a project with ignored local environment files - they are
      offered, copying works, and the new checkout runs
- [ ] 6.8 Remove a worktree with uncommitted changes - confirm names modified and
      untracked separately; after removal the branch still exists
- [ ] 6.8a Choose Keep the changes on that confirm - the worktree goes and the changes
      are recoverable where GitWyrm said they went
- [ ] 6.8b Remove a worktree whose only changes are untracked files - the confirm says
      they have never been saved anywhere
- [ ] 6.8c Delete a branch that a worktree holds - GitWyrm names the worktree, offers to
      remove it, and completes the branch deletion afterwards in one flow
- [ ] 6.8d Hold a file open in a worktree (an editor or a terminal sitting in the folder)
      and remove it - GitWyrm says a program is using the folder and Try again works once
      the holder is closed. The Windows-specific case, and the one most likely to be the
      app's own fault
- [ ] 6.8e Remove a worktree that is open as a GitWyrm tab and being watched - it
      succeeds, proving the app releases its own hold
- [ ] 6.8f Remove a worktree whose branch is merged - the follow-on branch-delete offer
      appears; repeat with unmerged work and confirm it does not
- [ ] 6.9 Turn the worktrees setting off in a repo that has worktrees - Add disappears,
      the existing worktrees stay listed and still open/remove/prune/repair, and the
      section says why Add is missing
- [ ] 6.10 With the setting off, create a worktree from a terminal while the GitWyrm
      window is open and focused - it appears on its own, the setting stays off, and
      the new worktree can be opened and removed from the section
- [ ] 6.11 With the setting off and no worktrees, confirm the section is absent
      entirely
- [ ] 6.12 Remove a worktree from a terminal while it is open as a tab - the row goes
      and the tab reports the folder is gone rather than showing stale contents
- [ ] 6.13 Run a task in its own worktree while editing files in the main checkout -
      the user's edits are undisturbed and the guardrail line names the run's folder
- [ ] 6.14 Discard an isolated run - folder gone, branch untouched
- [ ] 6.15 Discard an isolated run after hand-editing a file in its folder - GitWyrm
      offers to keep it, and the kept folder lists as an ordinary worktree
