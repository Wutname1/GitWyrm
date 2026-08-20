# Change: Add source-bound AI actions for issues and pull requests

## Why

The initiating use case is one click from real host work: Fix an issue, Review a pull
request, or Summarize it without rebuilding context in a blank chat.

## What Changes

- Add one shared source kickoff request and intent policy.
- Add issue Fix/Plan/Explain and PR Review/Summarize actions.
- Open/focus Agent Desk and create a Preparing session before slow work.
- Cache launch source data, enrich it in the background, and link to live host state.
- Require isolated worktrees for Fix while Review/Summarize remain read-only.

## Impact

- Extends `GithubContextPanel.tsx` and reusable host item menus.
- Adds host-neutral source snapshot commands/policy.
- Reuses current host providers, AI selection, run engine, and worktrees.
