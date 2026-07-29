# ai-ask Spec Delta

## ADDED Requirements

### Requirement: Ask is read-only by construction

Ask sessions SHALL be able to read the change package and the repository but SHALL
have no ability to modify anything - mutation capabilities are absent from the session,
not merely declined. No file, task, or git state changes during an ask session.

#### Scenario: Nothing changes

- WHEN a user asks questions for ten minutes
- THEN the working tree, tasks.md, and git state are byte-identical to before

### Requirement: Visibly distinct from runs

Ask mode SHALL look different from a run on purpose: chat bubbles instead of a step
stream, a persistent read-only banner ("the AI reads this change and the code, but
changes nothing"), no Stop button or state pill, the tab labeled "✦ Ask AI", and the
provider label suffixed "· read-only".

#### Scenario: No mode confusion

- WHEN the ✦ tab is open
- THEN a glance distinguishes an ask session from a run by layout, banner, and label

### Requirement: Grounded, cited answers

Answers SHALL be grounded in the change package (proposal, design, deltas) and the
code, and cite their sources as chips; clicking a chip SHALL jump to the matching Desk
tab.

#### Scenario: Why question

- WHEN the user asks why a behavior was chosen
- THEN the answer references the deciding document and its chip opens that tab

### Requirement: Explicit escalation to a run

When asked to make changes, Ask SHALL explain it cannot edit files and offer starting a
monitored run as a single explicit action. Promotion from read-only to write is always
one visible click, never silent.

#### Scenario: "Just do it"

- WHEN the user asks Ask to implement a task
- THEN the reply offers Start a run with AI, and nothing changes unless it is clicked

### Requirement: Ask shares session rules

Ask SHALL not start while a run is active (explain and route to the run), and its
pending replies SHALL be cancelled when any new session starts.

#### Scenario: During a run

- WHEN a run is working and the user clicks Ask about this change
- THEN the ask does not start and the user is pointed at the active run
