# Design

Agent Desk stores references to repository truth, not copied diffs. Result records identify
worktree, base/head commits, changed paths, check outcomes, and source/OpenSpec links. The
existing diff/query path renders current content.

Keep/Undo/Revise preserve current run-completion semantics. Commit is intentional and uses
source/OpenSpec trailers. Creating or updating a PR is a later explicit host action and
may require an explicit push confirmation supplied by the existing workflow; no agent
execution pushes.

Cleanup removes a run worktree only after work is safely integrated/discarded and no hand
edits exist. Crashes leave recoverable markers.
