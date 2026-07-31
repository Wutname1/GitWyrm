# specs-graph Specification

## Purpose
TBD - created by archiving change add-spec-commit-links. Update Purpose after archive.
## Requirements
### Requirement: Branch to change linking

A change SHALL be linkable to a branch, explicitly (Desk header or branch menu) or
inferred from `Spec:` commit trailers on that branch. Links SHALL be per-repository and
survive restarts.

#### Scenario: Explicit link

- WHEN the user links `feature/stash-in-graph` to change `stash-in-graph`
- THEN the commit form and graph treat that branch as linked immediately

#### Scenario: Unlink

- WHEN the user unlinks a branch
- THEN new commits stop receiving the trailer; existing commits keep their chips

### Requirement: Spec trailer on commits

On a linked branch, the commit form SHALL append a `Spec: <change-id>` trailer, shown
before committing and removable per-commit with one click. AI-generated commit messages
SHALL include the same trailer.

#### Scenario: Visible before commit

- WHEN the user is about to commit on a linked branch
- THEN the trailer is visible under the message, labeled as added automatically
- AND removing it affects only this commit

### Requirement: Spec chips in the graph

Commits whose message carries a `Spec:` trailer SHALL show a spec chip naming the
change. The linked branch's tip commit SHALL show the chip with live progress (`n/m`).
Clicking any chip SHALL open the Spec Desk at that change.

#### Scenario: Tip progress

- WHEN a linked branch's change has 7 of 10 tasks done
- THEN the tip commit's chip reads `<change-id> · 7/10`
- AND ticking a task updates it within one second

#### Scenario: Chip navigation

- WHEN the user clicks a spec chip
- THEN the Spec Desk opens (or focuses) with that change selected

### Requirement: AI commit attribution in the graph

Commits created by an in-app AI run SHALL carry an `Assisted-by:` trailer and display a
small ✦ AI marker on their graph row, so machine commits are always distinguishable
from the user's own.

#### Scenario: Marker

- WHEN an AI run commits
- THEN the new graph row shows the commit message with a ✦ AI marker and its spec chip

### Requirement: Graph reflects AI commits immediately

When an AI run commits, the graph SHALL show the new commit, move the branch ref to the
new tip, and increment the ahead count - without any manual refresh.

#### Scenario: Live update

- WHEN the AI commits on `feature/stash-in-graph` while the user watches the main window
- THEN a new row appears at the tip, the branch chip moves to it, and ↑2 becomes ↑3

