# Tasks

## 1. Shared contracts

- [ ] 1.1 Define `StartAgentSessionRequest`, typed source inputs, mode, team, and provider
      override in Rust/Specta.
- [ ] 1.2 Implement intent policy table and refusal of writes for read-only intents.
- [ ] 1.3 Add policy tests for all intent/mode/team combinations.
- [ ] 1.4 Add duplicate-session lookup by repo/source identity/intent/active state.
- [ ] 1.5 Register commands and regenerate bindings.

## 2. Frontend kickoff

- [ ] 2.1 Add `useStartAgentSession` shared by every source surface.
- [ ] 2.2 Add source-row Starting state before awaiting a command.
- [ ] 2.3 Open/focus Agent Desk and select the returned session immediately.
- [ ] 2.4 Keep failed preparation as a session with typed retry/reconnect/fallback action.
- [ ] 2.5 Clear Starting on all success/failure/unmount paths.

## 3. Issue actions

- [ ] 3.1 Add primary Fix with AI to issue detail/footer and context menu.
- [ ] 3.2 Add secondary Plan, Explain, and Fix with… actions.
- [ ] 3.3 Build the launch snapshot from loaded issue number/title/body/labels/assignee/URL.
- [ ] 3.4 Enrich comments and current state after Agent Desk is visible.
- [ ] 3.5 Derive branch/worktree suggestion through existing branch/worktree helpers.
- [ ] 3.6 Ensure closed/missing/read-only host states produce intentional action sets.

## 4. Pull-request actions

- [ ] 4.1 Add Review with AI and Summarize with AI to PR detail/footer and context menu.
- [ ] 4.2 Add Review with… as secondary override.
- [ ] 4.3 Snapshot PR metadata, head/base, draft/state, author, URL, and known checks.
- [ ] 4.4 Enrich commits/files/diffs/comments in Agent Desk using capability gates.
- [ ] 4.5 Prove Review/Summarize cannot call edit/worktree tools.
- [ ] 4.6 Escalating a review into a requested fix creates a new isolated execution linked to
      the same session/source.

## 5. Isolation and failure

- [ ] 5.1 Provision a marked worktree before the Fix engine receives edit capability.
- [ ] 5.2 If provisioning fails, do not fall back to the user's checkout.
- [ ] 5.3 Recover provider missing/reconnect, host offline, source deleted, branch held, and
      disk/path errors with typed cards.
- [ ] 5.4 Never push or post a host comment/review as part of kickoff.

## 6. Host coverage and proof

- [ ] 6.1 Test source identity/snapshot against GitHub, GitLab, Bitbucket, and Azure data.
- [ ] 6.2 Omit issue actions when host capabilities report no issue tracker.
- [ ] 6.3 Simulate two-second host/provider delays and assert immediate visible states.
- [ ] 6.4 Native-test Fix isolation and read-only Review/Summarize.
- [ ] 6.5 Run typecheck, Rust tests, and record Gate 3 evidence.
