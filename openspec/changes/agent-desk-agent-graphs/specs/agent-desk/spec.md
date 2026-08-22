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
The proposal turn itself SHALL be read-only. Start SHALL be the distinct authority
transition that permits worktree writes and helper launch.

#### Scenario: Plan graph

- WHEN a lead drafts a Plan graph
- THEN no helper starts until Start is chosen

#### Scenario: Plan proposes edits before Start

- WHEN a Plan-mode provider requests a file write while drafting the graph
- THEN the engine refuses the write and the user's checkout remains byte-identical

### Requirement: Graph integration is isolated from the user's checkout

Every graph SHALL have a dedicated lead integration worktree. Helper results SHALL be
combined there and SHALL NOT be copied into the repository checkout the user has open.
The integration worktree SHALL remain reviewable until an intentional Keep, landing, or
discard action.

#### Scenario: Helper completes while the user has local edits

- WHEN a helper result is integrated
- THEN the user's open checkout and local edits are unchanged and the combined change appears only in the integration worktree

### Requirement: Integration uses the helper's real worktree delta

Integration SHALL compare each helper worktree to its recorded base and include staged,
unstaged, and committed changes. A helper commit SHALL NOT be required for its result to
be integrated.

#### Scenario: Helper leaves an uncommitted edit

- WHEN a helper finishes with an uncommitted file modification
- THEN that modification is queued and applied to the graph integration worktree

### Requirement: Integration preserves repository file semantics

Integration SHALL preserve additions, modifications, deletions, renames, binary bytes,
symbolic links, and executable/file modes. A missing path or read failure SHALL NOT be
converted to an empty text file. Unsupported operations and failures SHALL pause visibly
with recoverable state and SHALL NOT report a complete combined result.

#### Scenario: Helper deletes and renames files

- WHEN a helper deletes one file and renames another
- THEN the same delete and rename are represented in the integration worktree and combined diff

#### Scenario: Binary file changes

- WHEN a helper changes a binary file
- THEN the exact bytes and file mode reach the integration worktree without UTF-8 conversion

### Requirement: Integration is serialized, conflict-aware, and recoverable

Completed helper deltas SHALL be integrated in a deterministic serialized order. A
conflict SHALL preserve the base, helper, and current integration versions, pause only the
affected integration, and resume from the recorded operation after the user resolves it.
The graph SHALL NOT claim Finished while an integration is partial, failed, or conflicted.

#### Scenario: Two helpers edit the same lines

- WHEN the second helper conflicts with the first integrated result
- THEN the graph shows the conflict and both helper results remain recoverable

### Requirement: Finished graphs have one combined reviewed result

After all helper deltas are integrated, the lead SHALL review the combined integration
worktree, run the required checks, and create one result that references the combined diff
and each helper result. A completed-node counter SHALL NOT substitute for this review.

#### Scenario: Last helper finishes

- WHEN the last helper is integrated successfully
- THEN the lead reviews the combined change before the graph can enter Finished

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
