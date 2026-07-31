# Tasks

## 1. Ask mode

- [x] 1.1 Driver read-only session (repo + change package readable; all mutation
      tools absent, not merely refused). Ask goes through `ai::complete`, the
      one-shot prompt path with no tool loop attached, so there is no edit
      capability to decline. A session that merely refused would still be one
      prompt injection away from writing.
- [x] 1.2 ✦ tab ask mode: bubbles, "Read-only — the AI reads this change and the code,
      but changes nothing" banner, no Stop, no state pill, tab label "✦ Ask AI".
      A run wins the tab when both exist: it has state to watch and can change
      files.
- [x] 1.3 Source chips on answers (proposal.md → Proposal tab, delta → Spec deltas tab).
      Chips are matched against the documents actually handed to the model, so a
      hallucinated filename cannot become a chip that opens nothing.
- [x] 1.4 Escalation reply with a Start-a-run button when asked to make changes.
      Shares `useStartRun` with the rail so both start the same run; the button is
      absent when no task is left, so it is never dead.
- [x] 1.5 Blocked while a run is active (explains, routes to the run). Starting a
      run also clears any ask session and bumps the epoch, dropping replies still
      in flight so none can land in the tab the run took over.

## 2. Verify

- [ ] 2.1 Ask about each change type (built, review, draft) — answers cite real sources
- [ ] 2.2 "Just do task 3 for me" produces the escalation, and nothing on disk changed
- [ ] 2.3 Ask → immediately start a run: no ask reply leaks into the run console
