# Agent Desk Spec Delta

## ADDED Requirements

### Requirement: A lead owns every graph

A multi-agent session SHALL have one lead responsible for the user conversation, source,
helper jobs, integration, and final response. Helpers SHALL not become separate primary
chats unless the user opens their detail.

#### Scenario: Helper finishes

- WHEN a helper completes
- THEN its result returns to the lead and the lead remains the conversation owner

### Requirement: Helpers are bounded and isolated

Every helper SHALL have its own execution ID, marked worktree, branch, allowed paths,
budget, and completion condition. Helpers SHALL never share the user's working directory.

#### Scenario: Two helpers edit

- WHEN two helpers write concurrently
- THEN each write exists only in its own worktree until intentional integration

### Requirement: Plan waits and Auto validates

Plan mode SHALL wait for Start after drafting a graph. Auto SHALL validate policy, graph,
and isolation before starting helpers. Solo SHALL not create a graph.

#### Scenario: Plan graph

- WHEN a lead drafts a Plan graph
- THEN no helper starts until Start is chosen

### Requirement: Stops have clear scope

Each helper SHALL have Stop for itself and the Graph header SHALL have a labeled Stop all.
Stopping SHALL preserve recoverable edits and respond promptly.

#### Scenario: Stop one

- WHEN the user stops one helper
- THEN peers continue and only that helper is cancelled

### Requirement: Graphs recover after restart

The graph SHALL be reconstructed from persisted backend execution state, including
working, waiting, stopped, failed, and conflicted nodes.

#### Scenario: Restart during approval

- WHEN GitWyrm restarts while one helper waits for approval
- THEN that gate and unaffected helper states reappear without rerunning completed work
