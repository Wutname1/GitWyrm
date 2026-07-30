# ai-runs Spec Delta

## ADDED Requirements

### Requirement: Starting a run

With an AI configured, a run SHALL be startable from the rail's next-task card, from
any open task row's hover action, and from the main-window spec card (which opens the
Desk in the same motion). Only one run per repository may be active; a second start
attempt SHALL explain this and route to the run tab. Starting SHALL be blocked while a
finished run awaits commit review.

#### Scenario: Start from a task row

- WHEN the user clicks Run with AI on task 7's row
- THEN the run targets task 7 specifically, not the next open task

#### Scenario: One at a time

- WHEN a run is active and the user starts another
- THEN nothing new starts and the message routes them to the active run

### Requirement: The run tab

An AI run SHALL live in a dedicated ✦ tab in the Desk center, with a status badge
(● working, ⏸ needs you, ✓ finished). Other tabs SHALL stay usable during a run - the
run is never modal. The tab's header SHALL always show the run's own change and task.

#### Scenario: Browse during a run

- WHEN a run is working and the user opens the Proposal tab
- THEN the proposal renders normally and the ✦ tab badge keeps reporting the run

#### Scenario: Header honesty

- WHEN the user selected a different change and returns to the run tab
- THEN the Desk header shows the run's change, matching the console beneath it

### Requirement: Guardrails are stated where the run happens

Every run SHALL state its guardrails in the console: works only on the linked branch (or
a new work branch when none is linked), one commit per task, never pushes, stoppable
anytime. The stated guardrails SHALL match actual behavior exactly - a run must never do
something its own header says it will not. Enforcement itself is specified by
`add-ai-agent-engine`; this requirement is that the promise is visible and honest.

#### Scenario: The promise is on screen

- WHEN a run is active
- THEN the console names the branch it is limited to and states that it never pushes

#### Scenario: Push is never offered

- WHEN any gate is presented
- THEN pushing is not among the choices - it is refused by the engine, not permitted by a
  click

### Requirement: Preflight is visible

Before editing anything, the run SHALL show a checklist of what it read: the plan
(proposal.md and design.md), the agreed behavior (each spec delta, or an honest "none
written yet"), the task and its done-means checks, and confirmation that the user's own
edits were set safely aside.

#### Scenario: No deltas honesty

- WHEN the change has no spec deltas
- THEN preflight says "Checked for spec deltas - none written yet", never claiming to
  have read documents that do not exist

### Requirement: Plain-language activity stream

The run SHALL narrate as plain-language rows - plan statements, file edits with change
counts and a View diff link into GitWyrm's own diff view, checks with pass or fail -
never a raw terminal dump. The stream SHALL scroll within its own container without
hijacking page or window scroll.

#### Scenario: Edit row

- WHEN the AI edits a file
- THEN a row shows the file, +/− counts, and View diff opens that edit in the diff view
  the user already knows

### Requirement: Thinking time is visible, not silent

A turn takes real time - the Claude Code spike measured 10 to 20 seconds for a realistic
diff, with startup only about a tenth of it. The console SHALL therefore show that the
run is working during a turn rather than sitting still, and Stop SHALL stay responsive
throughout. A silent gap long enough to read as a hang is a defect, not a wait.

Where the engine can stream partial output, the console SHALL show it as it arrives:
time-to-first-token is what makes a fifteen-second turn tolerable.

#### Scenario: Mid-turn

- WHEN a turn has been running for several seconds with nothing to report yet
- THEN the console shows the run is still working, and Stop still responds immediately

#### Scenario: Streamed output

- WHEN the engine can report a turn's output progressively
- THEN the console appends it as it arrives rather than waiting for the turn to finish

### Requirement: Approval gates

The run SHALL pause completely for side effects beyond plain in-repo edits and the
project's own checks: adding or removing a dependency, running installs, network
access, deleting files, or touching anything outside the repository. The gate card
SHALL name the consequence in plain words and offer exactly: Allow this once,
No - find another way (the AI visibly adapts), and Stop the run. No gate SHALL offer
"don't ask again" or require typing. While a gate is open, no further steps execute.

#### Scenario: Dependency gate

- WHEN the AI wants to add a library
- THEN the run pauses with a card naming the library and that it downloads code and
  changes package.json, until the user chooses

#### Scenario: Denial adapts

- WHEN the user chooses No - find another way
- THEN the next stream row states the alternative approach taken without the side effect

#### Scenario: In-repo edits are not gated

- WHEN the AI edits source files and runs the project's tests
- THEN no gate appears for those steps

### Requirement: Gates are mirrored everywhere

An open gate SHALL be visible from wherever the user is: the ✦ tab badge turns ⏸, the
rail shows an answer-needed banner with a View link, the status bar says the AI needs
an answer, and the main-window spec card switches to an amber "AI needs your OK" state
with an Answer-in-the-Spec-Desk action.

#### Scenario: Away in the main window

- WHEN a gate opens while the user is in the main window
- THEN the spec card and status bar surface it, and one click lands on the gate card

### Requirement: Stop is instant and safe

Stop SHALL be one click, always visible during an active run, and take effect
immediately - no delayed "stopping" state. The AI's edits so far are kept as
uncommitted changes; nothing is committed; the user's own prior work is untouched. The
stopped card SHALL offer Keep the edits, Undo the AI's edits, and Restart this task
(targeting the same change and task regardless of current selection).

#### Scenario: Stop at a gate

- WHEN the user stops the run from an open gate
- THEN the run ends immediately and the stopped card states nothing was committed

### Requirement: Failure is a dead-end for nothing

A failed run SHALL name its cause in one plain sentence, state that nothing was
committed and the user's own work is untouched, and offer: Keep the edits so far, Undo
the AI's edits, Restart this task, and Try again with a note. Provider sign-in failures
SHALL additionally offer Reconnect and the copy-handoff escape hatch.

#### Scenario: Checks kept failing

- WHEN the run gives up after repeated check failures
- THEN the card says so plainly and all four exits are offered

#### Scenario: Provider expired mid-run

- WHEN the provider's sign-in expires during a run
- THEN the card offers Reconnect and Copy task handoff so the user can finish elsewhere

### Requirement: Mid-run steering

A composer at the bottom of the run tab SHALL let the user send notes during a run.
Notes queue without interrupting the current step, appear in the stream as "You said:",
and are acknowledged in the AI's next step. An "Explain what you're doing" quick reply
SHALL produce a plain-language explanation row.

#### Scenario: Note mid-run

- WHEN the user sends "keep the dashed connector style" during a run
- THEN the stream echoes it and a following step acknowledges it

### Requirement: Session integrity

Ended or replaced sessions SHALL never emit output into a newer session's console. All
pending replies and timers from a previous run or ask session are cancelled when a new
session starts.

#### Scenario: Fast switch

- WHEN the user starts a run moments after asking a question
- THEN the pending ask reply never appears inside the run's console
