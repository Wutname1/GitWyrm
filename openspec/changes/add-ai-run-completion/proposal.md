# Change: Review-first run completion (validate, tick, commit)

## Why

The end of a run is GitWyrm's trust moment. Nothing is committed automatically: the run
finishes by proving itself (checks + spec check), ticking the task, and handing the
user a drafted commit to approve. The commit click stays the user's - that is what
makes an embedded agent acceptable in a git client.

## What Changes

- Completion sequence: project checks + `openspec validate` run automatically, the task
  is ticked in tasks.md (progress moves everywhere), then a Finished card
- Finished card: changed files with View diff, drafted commit message (house prefix +
  `Spec:` and `Assisted-by:` trailers), Commit these changes primary, Undo the AI's
  edits (which also un-ticks the task)
- Post-commit: graph updates (✦ row, branch tip, ahead count), History entry with
  approval count, explicit Start next task (never auto-chained)
- Ambient narration: main-window spec card live-narrates the run (working / needs-you /
  latest step) and returns to normal when idle

## Impact

- Affected specs: `ai-runs`
- Affected code: run driver completion path, Finished/committed cards, main-window
  spec card live states, History writer
- Depends on: `add-ai-task-runs` (the console), `add-ai-agent-engine` (the engine
  whose steps this completes), `add-spec-commit-links`
