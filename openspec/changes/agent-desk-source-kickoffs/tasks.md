# Tasks

## 1. Shared contracts

- [x] 1.1 Define `StartAgentSessionRequest`, typed source inputs, mode, team, and provider
      override in Rust/Specta. (`src-tauri/src/commands/agent_kickoff.rs`:
      `StartAgentSessionRequest`, `SessionSourceInput`.)
- [x] 1.2 Enforce read-only intent at provider launch and again at the runtime permission
      boundary. The provider process must start with write disabled (for Copilot ACP,
      `--deny-tool=write` or the version-gated equivalent), so remembered/global allow rules
      cannot bypass GitWyrm when no `PermissionRequest` is emitted. The existing
      `check_tool_capability` handler remains defense in depth, not the sole boundary.
      Reversed from the "second audit" note below, which is stale: `denied_tools_for`
      (`ai/agent/cli_agent.rs:65`) computes the deny set from policy/started, and
      `CliAgent::connect` (line 123) passes it straight into `AcpConnection::spawn`
      (`ai/agent/acp.rs:157`), which puts `--deny-tool={tool}` on the actual spawned process
      command line -- not merely a post-hoc `PermissionRequest` check. The code's own doc
      comment on `spawn` quotes `copilot help permissions`: "denial rules always take
      precedence over allow rules, even --allow-all-tools" -- this is precisely the hard
      boundary the "second audit" claims is missing. `shell_and_network_access_are_always_denied`
      and sibling tests in `cli_agent.rs` prove the set for every intent/started combination.
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
- [x] 2.3 Open/focus Agent Desk and select the returned session immediately. R3.3 landed a
      targeted event superseding the stale "relies on newest-session guess" note: backend
      `emit_select_session` (`src-tauri/src/commands/agent_kickoff.rs`) fires
      `agent-desk://select-session` with the exact `session_id` right after
      `agent_session_start` resolves; `AgentDeskView.tsx` listens for it (`listen<SelectSessionTarget>(SELECT_SESSION_EVENT, ...)`)
      and calls `setPaneSession(layout.activePane, ...)`, independent of the
      "land on newest" fallback effect (which now only covers the zero-pane-selected case).
- [ ] 2.4 Keep failed preparation as a session with typed retry/reconnect/fallback action.
      Kickoff's own `writeFailed` outcome is handled; the deeper provider/host recovery
      surfaces (`StartExecutionOutcome::ProviderReconnect`/`AdapterUnsupported`/
      `WorktreeFailed`) exist as typed backend outcomes but have no dedicated retry UI card
      yet -- native follow-up.
- [x] 2.5 Clear Starting on all success/failure/unmount paths. (`finally` block.)
- [x] 2.6 Start every explicit source operation after session creation. Fix, Plan, Explain,
      Review, and Summarize must not require a second composer Send; read-only intents run
      immediately with read-only authority.
      `agent_session_start` (`agent_kickoff.rs:557`) unconditionally calls
      `start_execution_at` for every `Created` session regardless of intent -- there is no
      per-intent gate left. Its own doc comment names the exact prior bug this replaced: the
      old frontend `IntentPolicy.canWrite` check skipped starting for Review/Summarize/Ask/
      Explain, leaving a Draft session that never ran. Read-only intents still cannot write
      (enforced by `check_tool_capability` and, per 1.2, `--deny-tool` at launch) but they do
      now run immediately, matching `useStartAgentSession.ts`'s own doc comment describing
      the same removal from the frontend side.
- [x] 2.7 Persist and carry the kickoff intent, mode, team, and provider override into the
      first execution. With no override, use the configured default provider; show an
      unsupported/unavailable choice instead of silently falling back.
      `resolve_kickoff_execution_params` (`agent_kickoff.rs:450`) resolves
      `request.mode.unwrap_or(intent_default)`/`request.team.unwrap_or(intent_default)`, and
      `request.provider_override` is passed verbatim into the same `start_execution_at` call
      that creates the session -- proven by `explicit_mode_override_is_not_replaced_by_the_intent_default`,
      `_team_override_...`, and `both_overrides_together_survive_independently`. The doc
      comment on `agent_session_start` names the exact prior bug: overrides used to be
      accepted into the request and then silently dropped before the old frontend auto-start
      call, which always passed the intent's plain default.

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
- [x] 3.5 Derive branch/worktree suggestion through existing branch/worktree helpers.
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
- [ ] 4.5 Prove live Review/Summarize cannot edit even when the provider has a remembered or
      global allow rule and never asks GitWyrm for permission. Assert the provider launch
      contains a hard write denial and native-test a dirty checkout byte-for-byte.
      PARTIAL, closer than the "second audit" note below suggests: the launch-time hard
      denial itself is proven (see 1.2 -- `--deny-tool=write` reaches the actual spawned
      process command line for Review/Summarize, per Copilot's own "denial always wins over
      allow rules" contract), and `explain_review_and_summarize_also_refuse_writes_before_asking`
      (`cli_run.rs:679`) drives a write request through the real production `handle()`
      function for these intents and asserts refusal before any gate is shown. What remains
      is specifically the native, real-repo, byte-for-byte dirty-checkout proof against a
      live provider process with a remembered allow rule -- that cannot be simulated in a
      unit test and needs an actual running app. Left unchecked for that native gap only.
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
- [ ] 5.5 Make concurrent start preparation transactional. If a final locked check finds a
      winner after this request provisioned a worktree, remove the unused worktree and branch
      unless they contain unique work; test the losing race leaves no orphan.
      PARTIAL, closer than the "second audit" note suggests: `cleanup_unused_worktree`
      (`agent_desk.rs:1058`) is real and wired at all 6 early-return points in
      `start_execution_at` between "worktree provisioned" and "engine launched" (including
      the exact re-checked-lock loss the task describes), proven in isolation by
      `cleanup_unused_worktree_removes_a_freshly_provisioned_worktree` and
      `cleanup_unused_worktree_refuses_the_main_checkout`. Separately,
      `concurrent_start_attempts_yield_exactly_one_started_and_one_already_running` proves the
      locking picks exactly one winner under real thread contention. But no single test
      connects the two -- an actual concurrent-start race that provisions a worktree, loses,
      and asserts the worktree is gone -- so "test the losing race leaves no orphan" is not
      literally met. Left unchecked for that missing end-to-end test.

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

## Status 2026-08-22 third audit

The "second audit" note below (2026-08-22) is itself stale/wrong on 1.2, 2.6, and 2.7,
re-verified against current code in this pass:

- 1.2: it claimed the runtime `PermissionRequest` refusal was the only boundary and provider
  allow rules could suppress it. That undersells the code: `denied_tools_for` /
  `AcpConnection::spawn` puts `--deny-tool=write` directly on the spawned process command
  line, which per Copilot's own documented contract (quoted in the code) takes precedence
  over every allow rule including `--allow-all-tools`. This is a hard launch-time boundary,
  not just a software permission check. Ticked.
- 2.6: it claimed source actions do not all start their selected operation. `agent_session_start`
  unconditionally starts every created session's execution regardless of intent -- the old
  frontend gate this concern describes was already removed, and the removal is documented in
  both `agent_kickoff.rs` and `useStartAgentSession.ts`'s own comments. Ticked.
- 2.7: it claimed kickoff overrides are discarded before the first run.
  `resolve_kickoff_execution_params` carries `request.mode`/`.team`/`.provider_override`
  verbatim into the same `start_execution_at` call, with three direct tests proving
  survival. Ticked.
- 4.5 and 5.5 are genuinely still open, but narrower than this note claimed: the *code*
  parts of both (hard deny-tool boundary; concurrent-start worktree cleanup) exist and are
  tested in isolation. What remains for each is a specific missing test/proof (a native
  dirty-checkout run for 4.5; an end-to-end losing-race-leaves-no-orphan test for 5.5), not
  an absent mechanism. See their own notes above.

## Prior status 2026-08-21 (historical)

Reconciliation pass ticked 1.2, 2.3, and 4.5 (all previously unchecked, all with real
production wiring found on re-read):

- 1.2: `check_tool_capability` is called from the real ACP `PermissionRequest` handler in
  `cli_run.rs::handle()`, before any gate is shown to the user, not just tested in isolation.
- 2.3: R3.3's targeted `agent-desk://select-session` event is emitted by
  `agent_kickoff.rs::emit_select_session` right after `agent_session_start` resolves, and
  consumed by a real listener in `AgentDeskView.tsx` that calls `setPaneSession`. This
  supersedes the file's own stale note about depending on a "land on newest" guess.
- 4.5: the policy unit tests already cited are joined by
  `explain_review_and_summarize_also_refuse_writes_before_asking`, which drives a write
  request through the actual production `handle()` function (not a mock) for
  Explain/Review/Summarize and asserts refusal before any gate. Still short of R1.8's native
  byte-identical dirty-checkout proof, which stays open under 6.4.

Everything else in this file was re-verified and left as-is:
- 2.4 and 5.3 correctly stay unchecked: `ProviderReconnect`/`AdapterUnsupported`/
  `WorktreeFailed` outcomes are surfaced only as toast text (`SessionComposer.tsx`,
  `AwaitingStartCard.tsx`), never as a retry/reconnect action control.
- 3.4, 4.4, and 4.6 correctly stay unchecked: `ConversationPane.tsx` has no source
  enrichment call and no escalate-review-to-fix path; grepped and confirmed absent.
- 6.3 and 6.4 correctly stay unchecked (no RTL harness; native item).
- No false-positive checkmarks were found in this file.
