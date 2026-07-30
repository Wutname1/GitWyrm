# Tasks

## 1. Design (do first)

- [ ] 1.1 Fold the "Claude Code CLI Provider for GitWyrm" spike into design.md: how the
      CLI is invoked, how its output is consumed turn by turn, how tool calls and results
      are exchanged, and how a run is cancelled mid-turn
- [ ] 1.2 Decide the Copilot CLI adapter shape against the same interface, and record
      what differs
- [ ] 1.3 Decide the turn budget and what "the task is done" means for the loop, so a run
      cannot spin forever

## 2. Provider CLI adapters

- [ ] 2.1 Detect each provider CLI the way git and gpg already are: configured path,
      PATH, then absent - with a plain-language message naming what to install
- [ ] 2.2 Claude Code adapter behind a `ProviderAgent` interface
- [ ] 2.3 Copilot CLI adapter behind the same interface
- [ ] 2.4 Surface a missing or broken CLI as the provider surface's reconnect state
      rather than a failed run

## 3. The loop

- [ ] 3.1 Plan / act / observe loop that ends when the task's done-means checks pass, the
      turn budget is spent, or the user stops
- [ ] 3.2 Tools, and only these: read file, edit file, list directory, run a project check.
      Every path resolved inside the repository and refused outside it
- [ ] 3.3 Emit the console's typed events as the loop progresses, each with its
      one-sentence plain-language summary

## 4. Guardrails (enforced here, not by the CLI)

- [ ] 4.1 Refuse any push outright - never a gate, never an option
- [ ] 4.2 Raise typed gates for side effects: add or remove a dependency, run an install,
      network access, delete files, anything outside the repository
- [ ] 4.3 Work only on the linked branch (or a new work branch when none is linked);
      refuse to run with a different branch checked out
- [ ] 4.4 Set the user's uncommitted work aside before the run and restore it after,
      reusing the stash plumbing
- [ ] 4.5 Cancel promptly on stop, including mid-turn, leaving the tree in a state the
      console's keep/undo choices can act on

## 5. Verify

- [ ] 5.1 A real task run end to end against Claude Code on a scratch repository
- [ ] 5.2 The same task against the Copilot CLI
- [ ] 5.3 Guardrails hold under a hostile prompt: ask it to push, to install a package,
      and to edit a file outside the repo - push refused, the others gated
- [ ] 5.4 Stop mid-turn leaves no half-written file and the user's own work intact
- [ ] 5.5 With no provider CLI installed, the Desk shows the reconnect state and the
      copy-handoff path still works
