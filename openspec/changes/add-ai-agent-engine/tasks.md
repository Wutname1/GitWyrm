# Tasks

## 1. Design

- [x] 1.1 Fold the "Claude Code CLI Provider for GitWyrm" spike into design.md: measured
      timings, the auth lesson, the numbers not to trust, and the version-gate finding
- [x] 1.4 Answer the terms-of-service question per provider. Done for Anthropic
      (prohibited in writing) and OpenAI (four unanswered asks; API keys recommended).
      Recorded in design.md, and the reason this change is API-key only
- [ ] 1.2 Resolve the Copilot question: GitWyrm already routes through GitHub's bundled
      Copilot CLI to obtain entitlements its own OAuth app is denied. Decide whether that
      stays, changes, or goes - it ships today
- [ ] 1.3 Decide the turn budget and what "the task is done" means for the loop, so a run
      cannot spin forever

## 2. Provider transports

- [ ] 2.1 `ProviderAgent` interface over each provider's documented API, keyed by the
      user's own API key. No subscription-credential path - see design.md
- [ ] 2.2 Anthropic implementation (Messages API with tool use)
- [ ] 2.3 OpenAI implementation behind the same interface
- [ ] 2.4 A provider with no key reports as unavailable in plain language, naming where to
      get one, without implying GitWyrm is broken
- [ ] 2.5 Cancellation terminates any in-flight request promptly

## 3. The loop

- [ ] 3.1 Plan / act / observe loop that ends when the task's done-means checks pass, the
      turn budget is spent, or the user stops
- [ ] 3.2 Tools, and only these: read file, edit file, list directory, run a project check.
      Every path resolved inside the repository and refused outside it
- [ ] 3.3 Emit the console's typed events as the loop progresses, each with its
      one-sentence plain-language summary
- [ ] 3.4 Stream a turn's output where the transport allows it, so the console has
      something to show inside the first second or two of a 10-20 second turn

## 4. Guardrails (enforced here, not by the provider)

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
- [ ] 5.3 The same task against a direct provider API with a key, proving the interface
      is not CLI-shaped
- [ ] 5.4 Guardrails hold under a hostile prompt: ask it to push, to install a package,
      and to edit a file outside the repo - push refused, the others gated
- [ ] 5.5 Stop mid-turn leaves no half-written file, no orphaned process, and the user's
      own work intact
- [ ] 5.6 With no provider CLI installed and no key, the Desk shows the reconnect state
      and the copy-handoff path still works
- [ ] 5.7 An account with credentials but a bad scope reports needs-reconnect rather than
      failing at generation time (the spike's own failure case)
