# Design

## Decisions

- **A worktree opens as its own repository tab, not a checkout switcher inside the
  existing tab.** A linked worktree is a real working folder with its own path, and
  `open_repo(path)` already discovers it, so tabs, the status bar, the file watcher,
  and every command that takes a `repo_id` work on it unchanged. The alternative - a
  checkout switcher inside one tab - would mean auditing every `repo_id` call site for
  "which checkout did the user mean", and it hides the thing the user most needs to
  know: which folder they are editing. Tabs already answer that.
  - Consequence: the main checkout and its worktrees are sibling tabs. Rows in the
    Worktrees section that are already open route to that tab instead of opening a
    second one.
  - Consequence: tab titles must disambiguate. A worktree tab is titled by its own
    folder name with the branch as the subtitle, the same shape as a repository tab.

- **`enableWorktrees` governs creation, not visibility.** The flag, its settings toggle,
  and the `has_worktrees` auto-enable already ship, and the flag keeps doing the job it
  was added for: a user who has never wanted worktrees does not grow a sidebar section.
  But a worktree that *exists* is part of the repository's real state, and hiding it
  behind a preference means the user cannot open, remove, or repair a checkout that
  another tool put there. So the flag gates Add; existence gates the section.
  - Consequence: three states, not two. Setting off + none: no section. Setting off +
    some exist: section with everything except Add, saying why Add is missing. Setting
    on: the full section.
  - Consequence: auto-enable becomes a first-encounter courtesy rather than the thing
    that makes worktrees visible. It must not fire after a deliberate off - and it no
    longer needs to, because the off state already shows what exists.

- **External changes are detected by watching, not by re-checking on activation.** The
  shipped `has_worktrees` call runs once when a repository becomes active, which cannot
  see a `git worktree add` typed in a terminal beside an already-focused window - and
  "nothing happened on screen" is exactly the failure the visible-change rule exists to
  prevent. The file watcher already planned for the list (task 2.4) covers the admin
  directory, so one watch serves both the list and the section's own appearance.
  - Consequence: the section can appear while the user is looking at the window, having
    never asked for it. That is correct - a new checkout of their repository showed up,
    and that is worth a row.

- **Default location is a sibling folder: `../<repo-name>-<branch-slug>`.** Outside the
  repository, so the checkout can never be committed into it; next to it, so it is
  findable without a file dialog hunt. The branch slug keeps sibling worktrees from
  colliding. The folder picker is always available - the default is a suggestion in a
  field, not a hidden decision.

- **Isolation is already mandatory for the agent room; this change adds it as an option
  for a single run.** `ai-runs` requirement "One worktree per agent" already says every
  agent in the room works in its own worktree and never in the user's directory - that is
  settled, and this change is where it actually gets built. The open question was only
  about *single* runs, and there the answer is opt-in: a single run already promises
  "your own work is untouched" and already delivers it by setting uncommitted changes
  aside in a stash before starting (`ai/agent/session.rs`). Making every single run create
  a folder on disk would trade a promise the user already has for one they must now clean
  up. Isolation earns its cost in one specific case - wanting to keep working in the same
  files while a run goes - so that is where it is offered.
  - Consequence: one worktree provisioning path serves both. The room asks for N of them
    and never asks the user; a single run asks for one only when the user ticked the box.
  - Consequence: `ai-runs` already requires that a worktree is never removed while it
    holds the only copy of an agent's work. The hand-edit rule below is the same
    principle applied to the discard path, not a competing one.
  - Consequence: the run's guardrail line must name the folder the run works in
    whenever that is not the user's own checkout. A run that says "works only on branch
    X" while editing a folder the user cannot see is exactly the header dishonesty
    `ai-runs` forbids.

- **A run worktree the user has touched is never silently deleted.** Discard removes the
  worktree only when its working tree is clean of anything the run did not write. If
  there are hand edits, discard says so and offers to keep the folder - the user chose
  to work there, and a discard is about throwing away the AI's result, not theirs.

- **Typed outcomes, not error strings.** Several conditions drive UI and must not arrive
  as text: a branch is checked out in another worktree (carrying *which* one, so the UI
  can offer to open it), a worktree is broken (folder missing vs. moved, since one prunes
  and the other repairs), removal refused for dirt (carrying modified and untracked counts
  separately), removal refused by the OS holding a file handle, and branch deletion
  refused because a worktree holds it (carrying which). Each of these has a different
  offer attached to it, so each has to be distinguishable without parsing git's message.

- **Removal is a chain, and the whole chain lives behind one confirm.** The most common
  worktree trap is not one refusal but three linked ones: the branch will not delete
  because a worktree holds it, the worktree will not delete because it is dirty, and on
  Windows the folder will not delete because a process has a handle in it. Each raw
  refusal names a symptom and no cause. GitWyrm resolves them in place - the branch-delete
  prompt offers to remove the worktree, the removal prompt offers keep-or-discard, and the
  lock failure offers Try again - so the user never leaves for a terminal to learn a flag.
  - `--force` is never a thing the user passes; it is what "discard them and remove"
    compiles to after a plain-language confirm.
  - Modified and untracked counts stay separate all the way to the UI. Discarding
    untracked files is the one case with no recovery, so it cannot be folded into a single
    "3 changes" number.

- **GitWyrm must release a worktree before removing it.** Windows will not delete a
  directory anything holds a handle in, and this app is a strong candidate for being that
  thing: it watches worktree paths (task 2.4) and can have one open as a tab. Every
  removal therefore stops watching and closes the repository handle for that path first.
  Nothing else can be fixed from inside the app - an editor or dev server in the folder is
  the user's to close - but the app being its own blocker is unforgivable, and it is the
  documented failure mode for tools in exactly this shape.

- **A new worktree contains only tracked files, which is why fresh ones look broken.**
  Ignored local config (environment files, editor settings) does not come along, and git
  says nothing about it. Offering to copy those - while explicitly refusing to copy large
  generated dependency folders, which are rebuilt rather than carried - turns the single
  most common "my worktree doesn't work" report into a checkbox at creation time.

- **git2 for reading, shell git for the state-changing paths.** `Repository::worktrees`
  and `find_worktree` cover listing and validity; `worktree_prune` covers pruning. Add,
  move, and repair go through shell git (`git worktree add|repair`), matching the
  existing local-vs-shell split and avoiding a hand-rolled reimplementation of what
  `git worktree add` does with the branch, the admin files, and the `.git` link file.

## Alternatives considered

- **Worktrees as a checkout switcher in one tab**: rejected above - hides the active
  folder and forces a `repo_id` audit.
- **Creating the worktree inside the repository (`.git/worktrees/..` sibling or a
  `worktrees/` subfolder)**: rejected. A working folder inside the working tree is a
  thing to gitignore correctly forever, and getting it wrong commits a whole checkout.
- **Isolation on by default for every run**: rejected above.
- **Reusing the run's stash-aside for isolation too**: rejected as the *only* option -
  it is the right default, but it cannot deliver "keep editing while it runs", which is
  the entire reason to build isolation.

## Open questions

- Whether a run's isolated worktree should be created on a temporary branch or a
  detached HEAD at the linked branch's tip. Detached avoids leaving branch clutter
  behind after a discard; a branch is easier to explain and to recover from if the app
  dies mid-run. Decide during 3.1 - the answer only affects the run path, not the
  worktrees capability itself.
