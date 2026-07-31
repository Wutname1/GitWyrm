# ai-ask Specification Delta

## ADDED Requirements

### Requirement: Asking for an edit

When the user asks Ask to change the wording of a change package file, Ask SHALL
offer to draft that edit rather than only explaining that it cannot write. The
draft SHALL name exactly one file of the change package.

Drafting SHALL NOT write to disk. The drafted text SHALL be returned to the Desk
and presented for review.

#### Scenario: An edit request is offered, not refused

- WHEN the user asks Ask to reword the Why section of the proposal
- THEN Ask offers to draft that edit, naming proposal.md

#### Scenario: Drafting touches nothing

- WHEN a draft has been produced but not accepted
- THEN the file on disk is byte-for-byte what it was before the question

### Requirement: A drafted edit is reviewed before it lands

A drafted edit SHALL be shown as a difference against the file currently on disk,
so the user sees what would change rather than only the proposed result.

Accepting a draft SHALL open it in the editor as unsaved text, never write it
directly. The user SHALL save it themselves. This makes an AI edit reach disk by
exactly the path a hand edit does, and leaves the AI's wording adjustable before
it lands. Rejecting SHALL leave the file untouched.

#### Scenario: The change is visible before it is real

- WHEN a draft rewrites two sentences of a proposal
- THEN the review shows those two sentences as changed, not the whole file as new

#### Scenario: Accepting is not saving

- WHEN the user accepts a drafted edit
- THEN the text opens in the editor marked unsaved, and the file on disk is
  unchanged until they save

#### Scenario: Rejecting is a complete undo

- WHEN the user rejects a drafted edit
- THEN nothing was written and the change package is as it was

### Requirement: Ask remains read-only

Ask's answering path SHALL remain a one-shot completion with no tool loop. Edit
drafting SHALL be a separate command with its own prompt and its own result type.

The Ask session SHALL continue to describe itself as read-only, because it is: no
answer, and no draft, changes a file on its own.

#### Scenario: The promise is structural

- WHEN any question is asked, including one demanding a file be rewritten
- THEN the answering call has no tool available that can write
