# Design

## Decisions

- **Read-only parsing is bundled; mutations prefer the CLI.** Viewing a repo's specs
  must never require installing anything. `validate` and `archive` have real semantics
  owned by the OpenSpec project, so GitWyrm shells to the CLI for those and reports a
  typed `CliMissing` outcome (with an install hint) instead of re-implementing them.
- **Sparse writes only.** The only files GitWyrm ever mutates are: a single checkbox in
  tasks.md, new-change scaffolds (later change), and archive moves via the CLI. The
  write-back reads the file, flips exactly one `- [ ]`/`- [x]`, writes it back. It never
  reformats. This is what keeps the GUI and any external editor/agent from fighting.
- **Watcher**: reuse the existing repo watcher infrastructure with an extra
  `openspec/**` glob rather than a second notify instance. Debounce 250ms.
- **Malformed files degrade, never crash.** A proposal without the expected headings
  renders as raw markdown; a tasks.md with odd formatting still counts whatever
  checkboxes it can find. Parse errors surface as a quiet per-change note, not a dialog.
- **Status derivation** (single source, used by both windows):
  `Draft` = no tasks or zero done; `In build` = some done; `Ready to archive` = all done
  and at least one delta; `Needs review` = all build-group tasks done but a group named
  like review/verify still open. Keep the rules in one Rust function with tests.

## Open questions

- Whether `changes/archive/` entries are parsed eagerly (archive list UI) or lazily on
  first open - start lazy, revisit if the archive view feels slow.
