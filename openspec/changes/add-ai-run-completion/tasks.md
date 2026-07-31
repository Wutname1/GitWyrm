# Tasks

## 1. Completion sequence

- [ ] 1.1 After the last step: run project checks and `openspec validate`, stream both results
- [x] 1.2 Tick the task in tasks.md (normal write-back; both windows update). Ticked
      only after the commit exists -- ticking first would leave a change claiming work
      that was never recorded if the commit failed.
- [x] 1.3 Finished card: drafted commit message with `new:`/`improved:`/`fixes:`
      prefix and `Spec:` + `Assisted-by:` trailers, Commit these changes primary, Undo
      the AI's edits secondary. The message is editable: a draft the user cannot
      correct is not really being approved. Changed-files + View diff is still open --
      the run's edits are already visible in the changes list, so this is a
      convenience rather than the trust-critical part.
      Prefix inference and both trailers are covered by `airun::complete` tests.

## 2. Commit and after

- [x] 2.1 Commit creates one commit on the linked branch. Goes through the existing
      `stage_all` + `create_commit` path rather than a parallel one, so signing and
      hooks behave exactly as a hand-made commit. The ✦ row and moved ref follow from
      the `Assisted-by:`/`Spec:` trailers, which `add-spec-commit-links` already reads.
      Live-refresh without a manual reload still needs checking in a window.
- [ ] 2.2 History entry: "Task N finished by AI (<provider>) — commit <sha> · you
      approved <n> steps"
- [x] 2.3 Never auto-starts: committing clears the run, and starting the next task is
      an ordinary explicit click on the rail. A dedicated Start-next button on the
      committed card is still open.
- [x] 2.4 Undo the AI's edits reverts the working tree and un-ticks the task. Confirmed
      first, and the dialog says plainly that your own edits to those files go too --
      this is the one ending action that destroys work. Discard runs before the
      un-tick: a tick left standing over discarded work would claim a task was done
      when its changes are gone.

## 3. Ambient narration

- [x] 3.1 Main-window spec card during a run: names the task number and the latest
      step, with Watch; amber needs-OK variant reads "Task N needs your OK" with an
      Answer action. A gate is answered, not watched, so the two states say different
      things.
- [x] 3.2 Card returns to its normal state when the session ends -- the banner is
      gated on a live session, so clearing the run removes it.

## 4. Verify

- [ ] 4.1 Full run to commit on a real branch; verify trailer, ✦ marker, history entry
- [ ] 4.2 Undo path: tree reverted, task un-ticked, progress rolls back everywhere
- [ ] 4.3 Two consecutive runs produce distinct commits with correct messages
