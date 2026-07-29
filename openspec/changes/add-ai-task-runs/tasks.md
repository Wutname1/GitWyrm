# Tasks

## 1. Run driver (backend)

- [ ] 1.1 Session model: one run per repo at a time; typed event stream
      (preflight / plan / edit / check / gate / note / error / finished-steps)
- [ ] 1.2 Provider adapter through the BYO-AI layer (Copilot first)
- [ ] 1.3 Guardrail enforcement: linked branch only, no push (hard-blocked), gated
      side effects (dependency add, install, network, delete, outside-repo)
- [ ] 1.4 The user's uncommitted work is set aside before the run and restored after
- [ ] 1.5 Stop: immediate cancel, working tree kept; structured failure outcomes
      (checks kept failing, provider auth expired, provider unavailable)

## 2. Run tab UI

- [ ] 2.1 ✦ AI tab appears with a session; status badge (● working / ⏸ needs you / ✓);
      other tabs stay usable during a run
- [ ] 2.2 Run header: "Task N · <text>", state pill (Working / Needs you / Finished /
      Stopped / Didn't finish), Stop always visible while active
- [ ] 2.3 Guardrail line naming the branch, one-commit-per-task, never-pushes, undoable
- [ ] 2.4 Preflight checklist (read plan / read deltas or "none yet" honestly / read
      task + done-means / your edits set aside)
- [ ] 2.5 Stream rows: plan, file edits with +/− counts and View diff (opens GitWyrm's
      own diff view), checks with pass/fail; stream scrolls its own container only
- [ ] 2.6 Steering composer (notes queue, never interrupt; echoed in stream;
      acknowledged next step) + "Explain what you're doing" quick reply

## 3. Gates

- [ ] 3.1 Inline amber gate card: consequence-first title, plain body, Allow this
      once / No — find another way / Stop the run; no don't-ask-again, no type-to-confirm
- [ ] 3.2 Run fully pauses at a gate (no background steps continue)
- [ ] 3.3 Denial visibly adapts (next step explains the alternative approach)
- [ ] 3.4 Mirroring: tab badge ⏸, rail banner with View link, status-bar "needs your
      answer", main-window spec card amber state

## 4. Stop and failure

- [ ] 4.1 Stop card: work kept as uncommitted changes; Keep / Undo the AI's edits /
      Restart this task (restart targets the run's own change + task)
- [ ] 4.2 Failure card: one-sentence cause, "nothing was committed and your own work is
      untouched", Keep / Undo / Restart / Try again with a note; auth failures add
      Reconnect and the copy-handoff escape hatch

## 5. Session integrity

- [ ] 5.1 One active run; second start attempts explain and route to the run tab
- [ ] 5.2 Replaced or ended sessions never emit into a new session's console
- [ ] 5.3 Opening the run tab always shows the run's own change in the header

## 6. Verify

- [ ] 6.1 Full run with an approve path and a deny path on a real branch
- [ ] 6.2 Stop mid-edit: working tree state verified, user's prior work intact
- [ ] 6.3 Gate mirroring visible from the main window; typecheck + unit tests for the
      driver's guardrail enforcement
