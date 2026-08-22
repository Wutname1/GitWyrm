# Tasks

## 1. Shared contracts

- [x] 1.1 Define `StartAgentSessionRequest`, typed source inputs, mode, team, and provider
      override in Rust/Specta. (`src-tauri/src/commands/agent_kickoff.rs`:
      `StartAgentSessionRequest`, `SessionSourceInput`.)
- [ ] 1.2 Implement intent policy table and enforce refusal of writes at the live engine/tool
      boundary for read-only intents.
      (`src-tauri/src/agentdesk/policy.rs`: `for_intent`, `check_tool_capability`.)
- [x] 1.3 Add policy tests for all intent/mode/team combinations. (`policy.rs` `mod tests`,
      9 tests including the exhaustive read-only proof.)
- [x] 1.4 Add duplicate-session lookup by repo/source identity/intent/active state.
      (`SessionSource::identity_key` in `model.rs`; `find_active_session_for_source` in
      `agent_kickoff.rs`.)
- [x] 1.5 Register commands and regenerate bindings. (`agent_session_start`,
      `agent_intent_policy` registered in `lib.rs`; bindings regenerated and verified.)

## 2. Frontend kickoff

- [x] 2.1 Add `useStartAgentSession` shared by every source surface.
      (`src/hooks/useStartAgentSession.ts`, used by `GithubContextPanel.tsx` and
      `LeftPanel.tsx`.)
- [x] 2.2 Add source-row Starting state before awaiting a command. (`setStartingKey` runs
      synchronously before any `await` in `startSession`.)
- [ ] 2.3 Open/focus Agent Desk and select the returned session immediately. (`openSpecDesk`
      is called concurrently with session creation; `AgentDeskView`'s existing "land on
      newest session" effect selects it once the list query invalidates.)
- [ ] 2.4 Keep failed preparation as a session with typed retry/reconnect/fallback action.
      Kickoff's own `writeFailed` outcome is handled; the deeper provider/host recovery
      surfaces (`StartExecutionOutcome::ProviderReconnect`/`AdapterUnsupported`/
      `WorktreeFailed`) exist as typed backend outcomes but have no dedicated retry UI card
      yet -- native follow-up.
- [x] 2.5 Clear Starting on all success/failure/unmount paths. (`finally` block.)

## 3. Issue actions

- [x] 3.1 Add primary Fix with AI to issue detail/footer and context menu.
- [x] 3.2 Add secondary Plan, Explain, and Fix with… actions. (Provider submenu in the
      context menu; Plan/Explain in both surfaces.)
- [x] 3.3 Build the launch snapshot from loaded issue number/title/body/labels/assignee/URL.
      (`src/lib/agentDeskSources.ts`: `issueSourceInput`.)
- [ ] 3.4 Enrich comments and current state after Agent Desk is visible. Covered structurally
      by the existing `agent_session_refresh_source` command and `SessionSourceBanner`
      cached/live states (owned by `agent-desk-conversation-shell`); no additional wiring
      added by this package.
- [ ] 3.5 Derive branch/worktree suggestion through existing branch/worktree helpers.
      `provision_kickoff_worktree` (task 5.1) derives the worktree at execution-start time;
      a pre-start branch-name *suggestion* shown in the UI before Fix is clicked was not
      built -- native follow-up if wanted.
- [x] 3.6 Ensure closed/missing/read-only host states produce intentional action sets.
      (Fix/Plan disabled on closed issues; issue actions omitted entirely when
      `capabilities.issues` is false, task 6.2.)

## 4. Pull-request actions

- [x] 4.1 Add Review with AI and Summarize with AI to PR detail/footer and context menu.
- [x] 4.2 Add Review with… as secondary override. (Provider submenu in the context menu.)
- [x] 4.3 Snapshot PR metadata, head/base, draft/state, author, URL, and known checks.
      (`pullRequestSourceInput`; "known checks" not yet surfaced by either `PrSummary`/
      `PrDetail` binding, so the snapshot text omits them honestly rather than fabricating.)
- [ ] 4.4 Enrich commits/files/diffs/comments in Agent Desk using capability gates. Not
      built by this package -- native follow-up alongside 3.4.
- [ ] 4.5 Prove the live Review/Summarize execution cannot call edit/worktree tools. (`policy.rs`:
      `review_and_summarize_cannot_call_edit_or_worktree_tools`,
      `read_only_intents_can_never_reach_a_write_or_worktree_tool`.)
- [ ] 4.6 Escalating a review into a requested fix creates a new isolated execution linked to
      the same session/source. Not built -- would live inside `ConversationPane.tsx`
      (owned by another in-flight package during this work); native follow-up.

## 5. Isolation and failure

- [x] 5.1 Provision a marked worktree before the Fix engine receives edit capability.
      (`agent_kickoff::provision_kickoff_worktree`, wired into
      `agent_desk::start_execution_at` step 2b so the engine's working directory is the
      worktree, not `open.path`, for any `WorktreePolicy::Always`/`NotUntilStart` intent.)
- [x] 5.2 If provisioning fails, do not fall back to the user's checkout.
      (`StartExecutionOutcome::WorktreeFailed` returns before the engine is ever started;
      proven by `provisioning_never_touches_the_users_own_checkout`.)
- [ ] 5.3 Recover provider missing/reconnect, host offline, source deleted, branch held, and
      disk/path errors with typed cards. The typed backend outcomes already exist
      (`StartExecutionOutcome`'s `ProviderReconnect`/`AdapterUnsupported`/`SourceMissing`/
      `WorktreeFailed`); dedicated recovery-card UI was not built -- native follow-up.
- [x] 5.4 Never push or post a host comment/review as part of kickoff. (Kickoff never
      constructs a `ToolCapability::Push`/`PostToHost` call; documented explicitly in
      `policy.rs`'s `fix_write_gate_does_not_by_itself_authorize_push_or_host_posts_at_kickoff`.)

## 6. Host coverage and proof

- [x] 6.1 Test source identity/snapshot against GitHub, GitLab, Bitbucket, and Azure data.
      (`identity_key_distinguishes_different_hosts_for_the_same_number` and friends in
      `model.rs`; `SessionSourceInput.hostId` is a plain string, never GitHub-specific.)
- [x] 6.2 Omit issue actions when host capabilities report no issue tracker. (Pre-existing
      `capabilities.issues` gate in `LeftPanel.tsx` already covers the whole Issues section,
      including the new AI actions.)
- [ ] 6.3 Simulate two-second host/provider delays and assert immediate visible states. No
      React test harness exists in this repo (vitest is Node-env only, no RTL) to drive
      `useStartAgentSession` under a simulated delay; the ordering guarantee (Starting set
      synchronously before any `await`) is structural in the hook's source, not test-proven
      here -- native follow-up (Gate 3's own native verification step).
- [ ] 6.4 Native-test Fix isolation and read-only Review/Summarize. Native in-app item.
- [ ] 6.5 Run typecheck, Rust tests, and record Gate 3 evidence. See verification output
      recorded in this change's implementation notes.
