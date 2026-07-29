# ai-runs Spec Delta

## ADDED Requirements

### Requirement: Review-first completion

A run SHALL finish by running the project's checks and `openspec validate`, streaming
both results, and marking the task done in tasks.md - and SHALL NOT commit anything
automatically. Progress updates from the tick appear in both windows immediately.

#### Scenario: Steps done

- WHEN the run's last step completes
- THEN the stream shows the checks and spec check passing and the task being marked done
- AND no commit exists yet

### Requirement: The finished card

The finished card SHALL present: the changed files with View diff links, a drafted
commit message using the house prefix style with `Spec: <change-id>` and
`Assisted-by: <provider>` trailers, a primary Commit these changes action, and an Undo
the AI's edits action that reverts the working tree and un-ticks the task.

#### Scenario: Commit is the user's click

- WHEN the finished card is showing
- THEN nothing lands in history until the user clicks Commit these changes

#### Scenario: Undo instead

- WHEN the user clicks Undo the AI's edits
- THEN the files return to their pre-run state, the task is open again in tasks.md,
  and progress rolls back everywhere

### Requirement: After the commit

Committing SHALL create exactly one commit on the linked branch. The graph SHALL show
it immediately (✦ marker, branch ref on the new tip, ahead count incremented). A
History entry SHALL record "Task N finished by AI (<provider>) - commit <sha> · you
approved <n> steps". The card SHALL offer Start next task as an explicit action; runs
are never auto-chained.

#### Scenario: Distinct consecutive commits

- WHEN two tasks are run and committed back to back
- THEN the graph shows two commits with distinct hashes and task-appropriate messages

#### Scenario: Never auto-chained

- WHEN a commit lands
- THEN the next run starts only if the user clicks Start next task

### Requirement: Ambient run narration in the main window

While a run is active on a change, the main-window spec card SHALL narrate it: a
working state with the latest step and a Watch in the Spec Desk action, and an amber
needs-your-OK state with an Answer action when a gate is open. When the session ends,
the card SHALL return to its normal state.

#### Scenario: Watching from the main window

- WHEN the AI is editing files while the user is in the main window
- THEN the spec card shows "● The AI is working on task N - <latest step>" and updates
  as steps complete
