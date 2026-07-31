# Change: Edit spec files, by hand or by asking

## Why

Reading a change in the Desk is a dead end. Every tab renders its file and offers
no way to change it - the reader who spots a wrong sentence in a proposal has to
leave for an outside editor, and the AI that just explained the problem cannot fix
it. Ask is read-only by construction, and a run only exists to tick off a task, so
a change with all its tasks done cannot be touched by the AI at all.

This closes both halves: a real editor in the Desk, and a way to ask the AI to
make an edit rather than only describe one.

## What Changes

- Every spec file in a change (proposal.md, tasks.md, design.md, each delta) is
  editable in place, in an embedded CodeMirror editor with markdown highlighting
- Edit is an explicit mode per tab, not an always-on textarea: tabs stay readable
  by default and become editable on a click, so a stray keystroke cannot alter a
  proposal someone was only reading
- Unsaved edits are held per file and survive tab switching, with a visible
  unsaved marker and a confirm before anything would discard them
- Ask gains one write escalation that does not exist today: "make this edit",
  which drafts a full replacement of one named file and shows it as a diff
  against what is on disk. Accepting opens the draft in the editor as unsaved
  text; the user saves it themselves
- Ask stays read-only. The drafting call is a separate command with its own
  prompt, and it returns text to the caller rather than touching the disk

## Impact

- Affected specs: `spec-desk` (editing), `ai-ask` (edit drafting)
- Affected code: Desk tabs (Proposal, Spec deltas, Overview/Tasks), a new editor
  component, `openspec_read_file` / `openspec_write_file` / `openspec_draft_edit`
- New dependency: CodeMirror 6 (`@codemirror/state`, `@codemirror/view`,
  `@codemirror/lang-markdown`, `@codemirror/commands`) - the first editor
  dependency in the app
- Depends on: `add-ai-ask-mode` (extends its escalation), `add-spec-desk-detail`
  (owns the tabs being made editable)

## Decisions worth flagging

**Ask does not gain edit tools.** `openspec_ask` goes through the toolless
one-shot path, and its own doc comment calls that promise structural rather than
behavioural. Adding a write tool there would dissolve it. Drafting is a separate
command that returns a proposed file body; only an explicit accept writes.

**Editing is scoped to the change package.** Writes resolve under
`openspec/changes/<id>/` and are refused elsewhere. The Desk is not a general
file editor, and a path escape here would be an editor bug with repository-wide
reach.

**An accepted draft opens in the editor rather than writing.** There is then one
write path in the app, not two: an AI edit lands by the same command, the same
path refusal, and the same save the user already knows. It also means the AI's
wording is adjustable before it is real, and that "accept" never surprises anyone
by being final.

**The draft diff is computed in TypeScript.** `commands/diff.rs` compares git
objects; a draft is an in-memory string against a working file, which is not that
shape. A line diff in TS avoids a Rust command that would exist only for this.
If it proves hard to read on real drafts, the fallback is a Rust command wrapping
the `similar` crate, whose word-level output would highlight changes inside a
reworded line rather than marking the whole line changed.

**CodeMirror over Monaco.** Monaco is VS Code's editor: several megabytes, its
own web workers, and an IDE's worth of chrome around files that are markdown
prose. CodeMirror 6 is roughly a tenth the size, needs no worker setup, and
themes from the tokens the app already defines.

It is still 504 kB (175 kB gzipped), which is more than this feature should cost
someone who never edits a spec file. So the editor is behind a dynamic import and
loads on the first Edit click, leaving the main bundle where it was.
