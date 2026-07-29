# Tasks

## 1. Completion sequence

- [ ] 1.1 After the last step: run project checks and `openspec validate`, stream both results
- [ ] 1.2 Tick the task in tasks.md (normal write-back; both windows update)
- [ ] 1.3 Finished card: changed files + View diff, drafted commit message with
      `new:`/`improved:`/`fixes:` prefix and `Spec:` + `Assisted-by:` trailers,
      Commit these changes primary, Undo the AI's edits secondary

## 2. Commit and after

- [ ] 2.1 Commit creates one commit on the linked branch; graph shows ✦ row, moved
      branch ref, bumped ahead count without refresh
- [ ] 2.2 History entry: "Task N finished by AI (<provider>) — commit <sha> · you
      approved <n> steps"
- [ ] 2.3 Committed card offers Start next task explicitly (targets the run's change);
      never auto-starts
- [ ] 2.4 Undo the AI's edits reverts the working tree and un-ticks the task

## 3. Ambient narration

- [ ] 3.1 Main-window spec card during a run: "● The AI is working on task N — <latest
      step>" with Watch in the Spec Desk; amber needs-OK variant with Answer action
- [ ] 3.2 Card returns to its normal state when the session ends

## 4. Verify

- [ ] 4.1 Full run to commit on a real branch; verify trailer, ✦ marker, history entry
- [ ] 4.2 Undo path: tree reverted, task un-ticked, progress rolls back everywhere
- [ ] 4.3 Two consecutive runs produce distinct commits with correct messages
