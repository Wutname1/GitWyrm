# Tasks

## 1. Driver seam

- [x] 1.1 `RunDriver` trait: start / event stream / answer_gate / note / stop, with typed
      events (preflight, plan, edit, check, gate, note, error, steps-done) and typed gate
      requests (AddDependency, RunInstall, NetworkAccess, DeleteFiles, OutsideRepo)
- [x] 1.2 Scripted driver behind the trait: replays a fixed event sequence with realistic
      timing, pauses at a gate until answered, and can be told to fail. Exists so every
      console state is verifiable before the engine lands - it is never presented to the
      user as a real run
- [x] 1.3 Session model: one run per repository at a time; every event carries the one
      plain-language summary the stream, the main-window card, and the status bar all
      render. Uses a global Tauri event rather than a channel: a gate has to be visible
      from wherever the user is, and a channel reaches only whoever opened it. Sessions
      are identified, not just replaced, so a driver still finishing cannot write into a
      newer run's console

## 2. Run tab UI

- [x] 2.1 ✦ AI tab appears with a session; status badge (working / needs you / finished);
      other tabs stay usable during a run
- [x] 2.2 Run header: "Task N · <text>", state pill with a glyph prefix (● ⏸ ✓ ■ ✕) so
      "Needs you" never reads as the change-status "Needs review", Stop always visible
      while active
- [x] 2.3 Guardrail line naming the branch, one-commit-per-task, never-pushes, undoable
- [x] 2.4 Preflight checklist (read plan / read deltas, honestly saying none when there
      are none / read task and its done-means / your edits set aside)
- [x] 2.5 Stream rows: plan, file edits with +/- counts and View diff, checks with
      pass/fail; the stream scrolls its own container only
- [x] 2.6 Steering composer: notes queue without interrupting, echo as "You said:", and
      an "Explain what you're doing" quick reply

## 3. Gates

- [x] 3.1 Inline amber gate card: consequence-first title, plain body, Allow this once /
      No - find another way / Stop the run; no don't-ask-again, no type-to-confirm
- [x] 3.2 The run fully pauses at a gate - no further steps execute
- [x] 3.3 Denial visibly adapts (the next step states the alternative taken)
- [x] 3.4 Mirroring: tab badge, rail banner with a View link, status bar, main-window
      spec card amber state. All four wired to the same event stream: the AI tab badge,
      a rail banner in the Desk, its own status-bar segment (its own, not appended to the
      openspec count -- a gate is something to act on, and a tally is how it gets
      missed), and the spec card, which turns amber and grows a View button that opens
      the run's own change

## 4. Stop and failure

- [~] 4.1 Stop card: work kept as uncommitted changes; Keep / Undo the AI's edits /
      Restart this task (restart targets the run's own change and task). Card and all
      three choices are built and say the right things. Keep closes the run out; Undo and
      Restart need the engine to have actually made edits, so they currently explain
      rather than act
- [~] 4.2 Failure card: one-sentence cause, "nothing was committed and your own work is
      untouched", Keep / Undo / Restart / Try again with a note; auth failures add
      Reconnect and the copy-handoff escape hatch. Card is built including the auth
      branch; Reconnect and copy-handoff handlers are not passed in yet

## 5. Session integrity

- [x] 5.1 One active run; a second start explains and routes to the run tab
- [x] 5.2 Replaced or ended sessions never emit into a newer session's console
- [x] 5.3 Opening the run tab always shows the run's own change in the header

## 6. Verify

None of section 6 is done. All three need the app running in a native window, and the
dev port was held by another session throughout this work. The console renders nothing
useful in a plain browser, where the Tauri IPC is absent -- confirmed, not assumed.

A dev-only demo launcher is in the Desk action rail (four scenarios plus Clear) so these
are quick to walk through once a native window is free.

- [ ] 6.1 Drive the scripted driver through every state on screen: working, gate open,
      denial, stopped, failed, provider-expired
- [ ] 6.2 Gate mirroring visible from the main window while the Desk is on another tab
- [ ] 6.3 Stop mid-run: the console says nothing was committed, and the choices work
