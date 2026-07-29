# ai-runs Spec Delta

## ADDED Requirements

### Requirement: The agent room

The Spec Desk SHALL support several monitored AI runs at once on a single change - one
agent per task - each using the same run grammar as single runs (visible preflight,
plain-language stream, approval gates, review-first completion) and each with its own
always-visible stop control, plus a room-level stop-all. Detailed scheduling, isolation,
and gate-queue requirements will be added by this change's spec-phase tasks before any
build task is written.

#### Scenario: Two tasks in parallel

- WHEN the user starts agents on two independent tasks of one change
- THEN each run shows its own state, stream, and stop control
- AND stopping one never affects the other

#### Scenario: Same trust rules

- WHEN any agent in the room hits a gated side effect
- THEN that run pauses on a normal approval gate while the others continue
- AND no agent may push, exactly as with single runs
