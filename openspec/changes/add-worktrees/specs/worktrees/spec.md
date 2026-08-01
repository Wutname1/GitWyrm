# worktrees Spec Delta

## ADDED Requirements

### Requirement: A worktree that exists is never hidden

A worktree that exists in the repository SHALL be listed and actionable regardless of
the worktrees setting. The setting governs whether GitWyrm offers to *create* worktrees,
not whether it admits the ones already there. A repository's real state is never withheld
from the person responsible for it, and a checkout the user cannot see is one they cannot
open, remove, or repair when it goes wrong.

Accordingly, while the setting is off and at least one linked worktree exists, the
section SHALL be shown with those worktrees listed, and the actions that manage what is
already there - open, remove, prune, repair - SHALL remain available. Only Add SHALL be
withheld. While the setting is off and no linked worktree exists, the section SHALL be
absent entirely.

#### Scenario: Another tool made one while the feature was off

- WHEN a worktree is created by another program in a repository whose worktrees setting
  is off
- THEN the section appears with that worktree listed, and it can be opened, removed,
  pruned, or repaired

#### Scenario: The setting still means something

- WHEN the section is shown because a worktree exists but the setting is off
- THEN Add is not offered, and the section says plainly that creating worktrees is
  turned off in settings

#### Scenario: Nothing to admit to

- WHEN the worktrees setting is off and the repository has no linked worktrees
- THEN the section is absent, and a user who has never wanted this feature never sees it

#### Scenario: Turning it off does not hide what exists

- WHEN the user turns the worktrees setting off in a repository that has worktrees
- THEN Add stops being offered, the existing worktrees stay listed and manageable, and
  none of them are removed from disk

### Requirement: The feature turns itself on for people already using it

The worktrees setting SHALL turn itself on the first time a repository is seen that
already has a linked worktree, so an existing worktree user never has to find the
toggle. It SHALL NOT turn itself back on after the user has deliberately turned it off -
their choice stands, and the previous requirement already guarantees they can still see
and manage whatever exists.

#### Scenario: First encounter

- WHEN a repository with a linked worktree is opened and the user has never touched the
  setting
- THEN the setting turns on by itself and the full section, including Add, appears

#### Scenario: A deliberate no is not overridden

- WHEN the user has turned the setting off and a worktree is later created by another
  program
- THEN the setting stays off, and the worktree is listed and manageable without Add

### Requirement: Worktrees made outside GitWyrm show up on their own

When a worktree is created, removed, or moved by another program while the repository is
open, GitWyrm SHALL reflect it without the user reopening the repository or switching
tabs. Detection SHALL NOT depend on the repository becoming active again, because the
common case is a terminal in the same folder as an already-open window.

#### Scenario: Created in a terminal

- WHEN the user runs a worktree command in a terminal while GitWyrm has the repository
  open and in view
- THEN the new worktree appears in the section without the user clicking anything

#### Scenario: Removed in a terminal

- WHEN a worktree is removed by another program
- THEN it disappears from the section, and if it was open as a tab that tab reports the
  folder is gone rather than showing stale contents

### Requirement: Worktrees are visible where branches are

Whenever the section is shown, the left panel SHALL list the repository's worktrees
alongside Branches and Submodules, showing each worktree's folder name, the branch it has
checked out, and which one is currently open. The main checkout SHALL be listed as one of
them, so the list is the whole picture rather than "the other ones". With the feature on,
a repository with no extra worktrees SHALL still show the section with a way to add one,
so the feature is discoverable before it is needed.

#### Scenario: Seeing what exists

- WHEN a repository has two worktrees
- THEN both are listed with their branches, and the open one is marked

#### Scenario: Nothing to list yet

- WHEN the feature is on and a repository has no extra worktrees
- THEN the section still appears with a plain-language line explaining what a worktree
  is and a way to add one

### Requirement: Adding a worktree explains where files will go

Adding a worktree SHALL ask for a branch (existing or new) and a folder, and SHALL
default the folder to a location outside the repository. The dialog SHALL state in plain
words that a new folder of working files is being created on disk, and SHALL name the
full path that will be created before the user commits to it.

#### Scenario: Default location is safe

- WHEN the add dialog opens
- THEN the suggested folder is outside the repository working tree, so the new
  checkout can never be committed into the repository by accident

#### Scenario: Choosing somewhere else

- WHEN the user picks their own folder
- THEN the dialog accepts it, and warns plainly if the chosen folder is inside the
  repository working tree

#### Scenario: The folder is already in use

- WHEN the chosen folder exists and is not empty
- THEN the dialog says so before anything is created, rather than failing partway

#### Scenario: Making a branch at the same time

- WHEN the user types a branch name that does not exist yet
- THEN the dialog offers to create it, saying which commit it will start from

#### Scenario: The result is visible

- WHEN a worktree is created
- THEN it appears in the section immediately, without a manual refresh

#### Scenario: A path that collides with a branch name

- WHEN the chosen folder name matches an existing branch name in a way git will reject
- THEN the dialog says so and suggests a different folder name before creating anything

### Requirement: A new worktree is not missing the files that make it work

A new worktree contains only tracked files, so the ignored ones a project needs to run -
local environment files, editor settings, credentials - are absent, and the checkout looks
broken in a way git never mentions. This is the most common reason a fresh worktree
appears not to work.

GitWyrm SHALL detect ignored files in the source checkout that a new worktree would lack,
and offer to copy them, listing what it found so the user chooses rather than being
surprised. Large generated directories such as dependency folders SHALL NOT be offered for
copying - they are rebuilt, not carried - and GitWyrm SHALL say plainly that they will need
rebuilding in the new folder.

#### Scenario: Environment files are offered

- WHEN a worktree is created from a checkout containing ignored local environment files
- THEN GitWyrm lists them and offers to copy them into the new folder

#### Scenario: Generated folders are not copied

- WHEN the source checkout has a large installed-dependency folder
- THEN it is not offered for copying, and GitWyrm says the new folder will need its
  dependencies installed before it will run

#### Scenario: Nothing to carry

- WHEN the source checkout has no ignored files worth copying
- THEN no copy step is shown, and creating a worktree stays a single confirm

### Requirement: The active worktree is never ambiguous

A worktree SHALL open as its own repository tab. When a worktree is open, the status bar
and the tab SHALL name it. Any action that changes files SHALL act on the worktree the
user is looking at.

#### Scenario: Two checkouts open

- WHEN the user has the main checkout and one worktree open and commits in one
- THEN the commit lands in the checkout named in that window's status bar, and the
  other is untouched

#### Scenario: Already open

- WHEN the user opens a worktree that is already open in another tab
- THEN GitWyrm switches to that tab instead of opening a second one

#### Scenario: Telling sibling tabs apart

- WHEN the main checkout and a worktree of the same repository are both open
- THEN their tabs are distinguishable by folder name and branch, not identical titles

### Requirement: A branch checked out elsewhere is explained, not errored

When a branch is already checked out in another worktree, GitWyrm SHALL explain that
in plain language, name the worktree that holds it, and offer to open that worktree,
instead of surfacing git's raw error text. This SHALL apply wherever a checkout can be
started, not only the branch list.

#### Scenario: Double checkout attempt

- WHEN the user tries to check out a branch that another worktree holds
- THEN GitWyrm says which worktree has it and offers to open that one

#### Scenario: Adding a worktree for a branch already taken

- WHEN the user picks a branch in the add dialog that another worktree already has
- THEN the dialog says so before creating anything, and offers to open that worktree

#### Scenario: No raw git text

- WHEN any worktree conflict is reported
- THEN the message is written for someone who has never read git's error output

### Requirement: Removing a worktree says what is lost

Removing a worktree SHALL use a plain-language confirm that states whether the
worktree has uncommitted changes and what happens to them. It SHALL NOT require
typing a name to confirm. Removing a worktree SHALL NOT delete the branch it had
checked out, and the confirm SHALL say so.

#### Scenario: Uncommitted work present

- WHEN the user removes a worktree with uncommitted changes
- THEN the confirm names the number of changed files before the user decides

#### Scenario: Nothing to lose

- WHEN the user removes a worktree with no uncommitted changes
- THEN the confirm says there is nothing unsaved in it

#### Scenario: The branch survives

- WHEN a worktree is removed
- THEN the branch it had checked out still exists and appears in the branch list

#### Scenario: Removing the one you are in

- WHEN the user tries to remove the worktree that is currently open
- THEN GitWyrm explains that it is the open one rather than removing the folder out
  from under the window

### Requirement: A worktree that will not delete is walked out, not refused

Removal is where worktrees most often trap people, and the trap is a chain: the branch
will not delete because a worktree holds it, the worktree will not delete because it has
changes in it, and the raw refusals name neither the cause nor the way out. GitWyrm SHALL
resolve each refusal in place rather than reporting it.

When removal is refused because the worktree has uncommitted changes, GitWyrm SHALL say
what is in there and offer the ways out as ordinary choices - keep the changes (setting
them aside so they are recoverable), or discard them and remove. Untracked files SHALL be
counted separately from modified ones, because "3 files you never saved anywhere" is a
different risk from "3 files you can get back from history".

Discarding SHALL be a normal plain-language confirm, never a flag the user has to know to
pass, and never a type-to-confirm.

#### Scenario: The dirty-worktree refusal

- WHEN removal is refused because the worktree has uncommitted changes
- THEN GitWyrm says how many files are modified and how many are untracked, and offers
  to keep them somewhere recoverable or to discard them, without the user learning a
  force flag

#### Scenario: Keeping the work

- WHEN the user chooses to keep the changes
- THEN the changes are set aside so they can be recovered later, the worktree is removed,
  and GitWyrm says where the work went

#### Scenario: Untracked files are called out

- WHEN the worktree's only changes are untracked files
- THEN the confirm says these have never been saved to history, since discarding them is
  the one case with no way back

### Requirement: Deleting a branch held by a worktree explains the real reason

When a branch cannot be deleted because a worktree has it checked out, GitWyrm SHALL name
that worktree and offer to remove it, rather than reporting that the branch is in use and
leaving the user to work out where. Removing the worktree from that prompt SHALL follow
the normal removal rules, including its own confirm when there are changes to lose.

#### Scenario: The branch that will not delete

- WHEN the user deletes a branch that a worktree holds
- THEN GitWyrm names the worktree holding it and offers to remove that worktree first

#### Scenario: The chain resolves in one place

- WHEN the user accepts removing the worktree from that prompt and it has uncommitted
  changes
- THEN the removal confirm appears with its keep-or-discard choice, and on completion the
  branch deletion the user originally asked for goes ahead

### Requirement: A folder Windows will not release is explained, not silently failed

On Windows a folder cannot be deleted while a process holds a handle in it, so removing a
worktree can fail for a reason that has nothing to do with git: an editor, a terminal, a
dev server, a build watcher, or a virus scanner is still in it. GitWyrm SHALL recognise
this case, say plainly that something is still using the folder, and offer to try again -
rather than reporting a permission error the user cannot act on.

GitWyrm SHALL close its own hold on a worktree before removing it, so the app is never
itself the reason removal fails.

#### Scenario: Something else is holding it

- WHEN removal fails because a file in the worktree is in use
- THEN GitWyrm says a program is still using the folder, suggests closing editors or
  terminals open in it, and offers Try again

#### Scenario: GitWyrm is not the culprit

- WHEN the user removes a worktree that is open in a GitWyrm tab or being watched
- THEN GitWyrm releases it first, so its own file watching never causes the failure

#### Scenario: Partial removal is not left silent

- WHEN removal deletes some files and then fails
- THEN GitWyrm says the folder was only partly removed and what state it is in, rather
  than reporting plain success or plain failure

### Requirement: Removal offers to tidy up what it leaves behind

Removing a worktree leaves its branch, and often that branch is finished with too. After a
successful removal GitWyrm SHALL offer to delete the branch as a follow-on choice when it
is safe to - not when the branch has commits that exist nowhere else. Cleaning up in two
places is the step people forget, and forgetting it is how a branch list fills with dead
names.

#### Scenario: Finished with the branch too

- WHEN a worktree is removed and its branch is fully merged
- THEN GitWyrm offers to delete the branch as well, saying it is merged and safe to remove

#### Scenario: Unmerged work is not offered away

- WHEN a worktree is removed and its branch has commits that are not merged anywhere
- THEN GitWyrm does not offer to delete the branch, and says the work is still on it

### Requirement: Worktrees broken outside GitWyrm can be repaired

When a worktree's folder has been moved or deleted outside GitWyrm, the section SHALL
show it as broken and offer the action that fits: repair when the folder was moved,
prune when it is gone. The row SHALL say which case it is in plain words rather than
offering both and letting the user guess.

#### Scenario: Folder deleted in Explorer

- WHEN a worktree folder is deleted outside the app
- THEN it appears as broken with a prune action that cleans up the reference

#### Scenario: Folder moved in Explorer

- WHEN a worktree folder is moved outside the app
- THEN it appears as broken with a repair action that points git at the new location

#### Scenario: Repair needs the new location

- WHEN the user repairs a moved worktree
- THEN GitWyrm asks where it went, and afterwards the row lists normally again

#### Scenario: Deleting the folder by hand is a normal thing to have done

- WHEN a worktree folder was deleted by hand rather than removed through GitWyrm
- THEN the leftover reference is presented as tidying up, not as an error the user
  caused, and pruning it is one click

#### Scenario: The whole repository moved

- WHEN the main repository folder is moved and every worktree's link to it breaks at once
- THEN GitWyrm explains that moving the repository broke the links and offers to repair
  them together, rather than showing a list of separately broken rows

#### Scenario: Broken does not mean gone

- WHEN a worktree is shown as broken
- THEN GitWyrm says whether the files are still on disk, so the user knows whether
  anything is at risk before choosing an action

### Requirement: A run can work in its own worktree

Starting a single Spec Desk run SHALL offer running the task in its own worktree. The
option SHALL be off by default for a single run; the agent room requires it always and
does not ask. When it is on, the run works in that checkout, the user keeps working in
theirs, and the result SHALL be reviewed as a diff before it reaches the user's branch.
The run console SHALL name the folder the run is working in whenever that is not the
user's own checkout.

#### Scenario: Working during a run

- WHEN a task runs in its own worktree
- THEN the user's working tree is unchanged while the run is in progress, and they can
  edit and commit in it normally

#### Scenario: Off by default

- WHEN the user starts a run without choosing isolation
- THEN the run works in the user's own checkout with their edits set aside as before,
  and no folder is created on disk

#### Scenario: The console says where it is working

- WHEN a run is working in its own worktree
- THEN the guardrail line names that folder, so the run never claims to be editing
  files the user is looking at

#### Scenario: Reviewing before it lands

- WHEN an isolated run finishes
- THEN its result is presented as a diff to review, and nothing reaches the user's
  branch until they accept it

### Requirement: Discarding an isolated run never throws away hand-written work

Discarding a run that worked in its own worktree SHALL delete that worktree when it
holds only what the run produced. When the user has edited files in it by hand, discard
SHALL say so and offer to keep the folder instead of deleting it.

#### Scenario: Discarding an isolated run

- WHEN the user discards a run that worked in its own worktree and did not touch it
  themselves
- THEN the worktree folder is removed and the user's branch is untouched

#### Scenario: The user edited the run's folder

- WHEN the user discards such a run after editing files in that worktree by hand
- THEN GitWyrm says the folder has their own edits in it and offers to keep the folder,
  and keeping it leaves a normal worktree listed in the section

#### Scenario: A leftover folder is still a worktree

- WHEN a run's worktree is kept after a discard
- THEN it behaves as any other worktree: listed, openable, and removable on its own
