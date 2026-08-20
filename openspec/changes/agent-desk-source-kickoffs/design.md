# Design

## Intent policy

Policy is backend-owned and exhaustively tested. Fix defaults to Auto + Lead and always
isolates. Review, Summarize, and Explain default to Ask + Solo and cannot write. Plan may
inspect but cannot execute before Start.

## Feedback order

The click first marks the source Starting, then focuses Agent Desk, creates/selects a
durable session, and shows Preparing. Provider resolution, full detail fetch, and worktree
creation happen after visible acknowledgement.

## Deduplication

An active session with the same repo/source/intent is focused and explained. A finished
session may be reopened or a new session explicitly started. Never silently start two
Fix executions for one issue.

## Host neutrality

UI wording follows host capabilities. The implementation uses existing `HostProvider`
data and does not introduce GitHub-only backend session types.
