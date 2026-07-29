# Design

## Decisions

- **Driver abstraction, opencode first-class.** The run driver is a trait
  (`start / event stream / answer_gate / note / stop`) so the engine underneath can be
  opencode in headless/server mode, a direct provider loop, or a future ACP client.
  UI code never knows which engine ran.
- **Gates are a protocol, not UI copy.** The driver emits typed gate requests
  (`AddDependency { name }`, `RunInstall`, `NetworkAccess { host }`, `DeleteFiles`,
  `OutsideRepo`) and blocks until answered. `Push` is not a gate - it is refused in the
  driver unconditionally. Plain in-repo edits and the project's own checks are never
  gated: they are reviewable and undoable, and gating them trains blind clicking.
- **Setting work aside**: the user's uncommitted changes are stashed to a GitWyrm-owned
  ref before the run and restored after (reusing the stash plumbing). This is what makes
  the "your own work is untouched" promise literally true and Stop always safe.
- **Event stream over IPC**: run events go over a Tauri channel; the UI appends rows.
  Every event carries a short plain-language summary used by the stream, the main-window
  card narration, and the status bar - one string, three surfaces.
- **State machine** (single enum, both windows render from it):
  `Working → NeedsYou (gate) → Working → StepsDone` plus `Stopped` and `Failed{cause}`
  from any active state. Completion states live in `add-ai-run-completion`.
- **Pill vocabulary**: run-state pills always carry a glyph prefix (● ⏸ ✓ ■ ✕) so the
  amber "Needs you" never reads as the amber change-status "Needs review".

## Alternatives considered

- Embedding a terminal emulator showing raw agent output: rejected - contradicts the
  plain-language rule and makes gates unenforceable.
- Auto-approving "safe" dependencies: rejected - the pause is the safety story.

## Open questions

- Which engine ships first behind the trait (opencode headless vs direct loop) -
  decide during 1.2 after spiking the opencode server protocol.
