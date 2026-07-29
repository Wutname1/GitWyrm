# ai-authoring Spec Delta

## ADDED Requirements

### Requirement: Draft it for me

With an AI configured, the new-change flow SHALL offer drafting the change from a
plain-language description: the AI reads the specs library, recent commits, and the
description (stated to the user), and produces a proposal, a task breakdown, and spec
deltas as a draft. The folder name SHALL remain user-chosen; name collisions are
auto-uniqued visibly, never dead-ending the flow.

#### Scenario: Second draft with the same name

- WHEN a drafted change would collide with an existing folder name
- THEN the name is uniqued (visible to the user) and the flow continues to review

#### Scenario: No AI

- WHEN no AI is configured
- THEN the new-change flow shows only Start blank, with a hint that connecting an AI
  adds drafting

### Requirement: Drafting is staged and cancellable

Drafting SHALL show its stages completing (reading the specs library, drafting the
proposal, breaking into tasks, writing deltas) rather than a spinner, complete within a
short bounded time, and be cancellable at any point with nothing written.

#### Scenario: Cancel mid-draft

- WHEN the user cancels during drafting
- THEN the dialog closes and `openspec/` is untouched

### Requirement: Review before anything is written

The draft SHALL be presented as per-artifact cards (Proposal, Tasks, Spec deltas) with
previews and Keep/Skip controls that genuinely determine what is created. A live count
SHALL state how many files Create will write. Nothing SHALL be written to disk until
Create; Discard SHALL leave no trace; Create with zero kept parts SHALL be blocked with
a hint.

#### Scenario: Skip is real

- WHEN the user skips the Spec deltas card and creates
- THEN the created change has no delta files and the count said "writes 2 files"

#### Scenario: Files only on Create

- WHEN the review screen is open
- THEN no files exist yet; they appear only after Create, and the change is then
  selected in both windows

### Requirement: Authoring provenance

Every AI-drafted artifact SHALL be attributed in the change's History in the form
"Drafted with <provider> · reviewed by you". The human is always the author of record;
AI is labeled help.

#### Scenario: History start

- WHEN a drafted change is created
- THEN its first History entry reads "Drafted with <provider> · reviewed by you"

### Requirement: Validate-fix loop

When a spec check fails for a missing requirement and an AI is configured, the result
SHALL offer Fix with AI: the AI drafts a requirement from that change's own proposal,
shown as a review card with the full delta text and Add this delta / Dismiss choices.
Adding writes the delta, updates the Spec deltas tab and History, and attaches to the
change that was checked - even if the user switched selection while it drafted.

#### Scenario: Fix and pass

- WHEN the user adds the drafted delta and re-runs the check
- THEN the check passes and History records the drafted delta with attribution

#### Scenario: Selection moved mid-fix

- WHEN the user selects another change while the fix drafts
- THEN accepting still attaches the delta to the originally checked change
