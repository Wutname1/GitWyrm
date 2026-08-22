# Agent Desk implementation architecture

## 1. Reuse boundaries

Do not replace these systems:

- `src-tauri/src/airun/` remains the execution engine and event producer.
- `src-tauri/src/ai/agent/` remains transport/provider selection.
- `src-tauri/src/hosting/` remains the host-neutral source of issues and pull requests.
- `src-tauri/src/git/worktree.rs` remains execution isolation.
- `src-tauri/src/openspec/` remains the parser and writer for OpenSpec files.
- GitWyrm's current diff view remains the changed-file renderer.

Agent Desk adds a durable session layer above them and view adapters below them.

## 2. Persistence

Use one JSON file per session under the Tauri app-data directory, not `settings.json`:

```text
agent-desk/
  v1/
    index.json
    sessions/
      <session-id>.json
    imports/
      <adapter-id>.json
```

Reasons:

- It follows existing app-data patterns without adding a database dependency.
- A damaged session does not destroy the entire history.
- Individual files can be migrated and backed up.
- Dozens of sessions per day remain small enough for a compact index.

Write to a sibling temporary file, flush, then rename. Rebuild `index.json` from session
headers if it is missing or invalid. Never rewrite session history from frontend state.

### Rust types

Create `src-tauri/src/agentdesk/model.rs` with serde and Specta types:

```rust
type SessionId = String;
type MessageId = String;
type SegmentId = String;

struct AgentSessionHeader {
    schema_version: u16,
    session_id: SessionId,
    repo_id: String,
    repo_path: String,
    repo_name: String,
    title: String,
    source: SessionSource,
    intent: SessionIntent,
    state: SessionState,
    created_at: String,
    updated_at: String,
    unread: bool,
    changed_file_count: u32,
    active_execution_id: Option<String>,
}

struct AgentSession {
    header: AgentSessionHeader,
    segments: Vec<ConversationSegment>,
    messages: Vec<SessionMessage>,
    executions: Vec<ExecutionRecord>,
    attachments: Vec<ContextAttachment>,
}
```

`SessionSource` is a tagged enum. Every variant carries a cached title/body summary,
captured timestamp, and optional live locator:

- `Manual { repo_id }`
- `Issue { host_id, owner, repo, number, url, snapshot }`
- `PullRequest { host_id, owner, repo, number, url, head, base, snapshot }`
- `OpenSpecChange { change_id, snapshot }`
- `OpenSpecTask { change_id, task_index, task_text, snapshot }`
- `Commit { oid, snapshot }`
- `Diff { scope, paths, snapshot }`
- `WorkingChanges { paths, snapshot }`
- `CheckFailure { provider, check_id, url, snapshot }`

Never use a bag of optional fields. Exhaustive variants force every source type to define
refreshing, display, and kickoff behavior.

`SessionIntent`: `Ask | Explain | Plan | Fix | Review | Summarize`.

`SessionState`: `Draft | Preparing | Ready | Working | NeedsInput | Finished | Failed |
Stopped | MissingSource`.

`SessionMessage` includes:

- stable ID, segment ID, role, timestamp, plain/rendered content;
- provider/model metadata when known;
- kind: user, assistant, thought-summary, tool, approval, system, or result;
- optional execution ID and run-event sequence;
- import provenance; and
- optional target IDs for file, diff, source, graph node, or OpenSpec task.

Persist provider-visible reasoning only when the provider actually emits content intended
for clients. Never label fabricated summaries as hidden thoughts.

## 3. Commands

Add `src-tauri/src/commands/agent_desk.rs` and register it in `lib.rs`:

- `agent_session_list(filter, cursor, limit)`
- `agent_session_get(session_id)`
- `agent_session_create(request)`
- `agent_session_append_user_message(session_id, content, attachments)`
- `agent_session_rename(session_id, title)`
- `agent_session_archive(session_id)`
- `agent_session_mark_read(session_id)`
- `agent_session_refresh_source(session_id)`
- `agent_session_start_execution(session_id, mode, team, provider_override)`
- `agent_session_stop_execution(session_id, scope)`
- `agent_session_usage(session_id)`
- `agent_import_scan(adapter_id)`
- `agent_import_session(adapter_id, external_session_id)`
- `agent_config_scan()`
- `agent_config_preview_copy(item_id, destinations)`
- `agent_config_apply_copy(plan_id)`
- `agent_config_undo(operation_id)`

Every mutation returns a typed outcome. Expected states such as already running, source
missing, adapter unsupported, provider reconnect, and conflicting write are enum variants,
not error strings.

Run `cargo run --manifest-path src-tauri/Cargo.toml --bin export_bindings` after command
registration or type changes.

## 4. Backend event bridge

Keep `ai-run-event` for compatibility while adding `agent-session-event` with:

```rust
struct AgentSessionEvent {
    session_id: String,
    execution_id: Option<String>,
    sequence: u64,
    occurred_at: String,
    kind: AgentSessionEventKind,
}
```

The bridge maps existing `RunEventKind` into durable session messages and then emits the
session event. Persistence happens before emission so a window crash cannot show content
that was never saved. Ignore an event when its execution ID is no longer active. Sequence
numbers make duplicates harmless and expose gaps.

## 5. Frontend stores and queries

Create focused state instead of expanding `workspaceStore`:

- `src/stores/agentDeskUiStore.ts`: workspace layout, selected session per pane, active
  pane, dock placement, source visibility, sidebar grouping, and session-keyed drafts.
  Persist only harmless UI preferences; keep open popovers and drag state ephemeral.
- `src/stores/agentSessionStore.ts`: live event merge keyed by session ID and sequence.
- `src/hooks/useAgentSessions.ts`: paged headers and filters through TanStack Query.
- `src/hooks/useAgentSession.ts`: one session plus event subscription.
- `src/hooks/useStartAgentSession.ts`: immediate create/focus/preparing sequence shared by
  every source action.

Do not keep canonical session content only in Zustand. Commands own durable data; queries
load it; the store overlays live events.

The workspace state is a view over sessions, not another session store:

```ts
interface AgentWorkspaceLayout {
  split: boolean
  activePane: 'primary' | 'secondary'
  primarySessionId: string | null
  secondarySessionId: string | null
  sourceBarsVisible: boolean
  dock: null | {
    kind: 'source' | 'context' | 'graph'
    edge: 'left' | 'right' | 'bottom'
    leftOrder?: 'above-chats' | 'below-chats'
    sizePx: number
  }
}
```

Composer drafts are keyed by session ID. Replacing a pane cannot discard a draft, and
Send clears it only after the user event is accepted. A pane stores its session ID and
viewport state; it never owns or copies the conversation.

## 6. Window migration

Extend `WindowMode` with `kind: 'agent-desk'`. Continue accepting
`?window=spec-desk` and translate it to Agent Desk with an OpenSpec source/filter.

The backend opens one stable app-wide Agent Desk label. During migration, lookup checks
legacy per-repository Spec Desk labels so an existing Desk is focused and migrated rather
than duplicated. A kickoff from another repository selects that session in the same
window instead of creating another Desk.

Recommended component move, done mechanically in one task:

```text
src/views/SpecDeskView.tsx                 -> src/views/AgentDeskView.tsx
src/components/domain/spec-desk/          -> keep temporarily, then move by feature
src/lib/specDesk.ts                        -> thin compatibility wrapper over agentDesk.ts
```

Do not rename every file before behavior works. First route both names to the new shell;
move components in later mechanical commits.

## 7. UI component map

```text
AgentDeskView
  AgentDeskTitleBar
  AgentWorkspaceToolbar
  SessionSidebar
    NewSessionButton
    SessionGroupingTabs
    VirtualSessionList
    ProjectSessionGroup
  ConversationWorkspace
    ConversationPane (one or two)
      PaneDetailButtons
      SessionSourceBanner
      ConversationTranscript
      MessageHistoryRail
      SessionComposer
        OperatingModeControl
        TeamShapeControl
  SessionDetailHost
    PaneDetailPopover
    DockedDetailPanel (left, right, or bottom)
      SessionSourcePanel
      SessionContextPanel
      AgentGraphPanel
  AgentSetupView
```

Use virtualized rows once 100 sessions are loaded. Each row is one line, 28-32 px high,
with icon, ellipsized title, and time/status. Project group headers must not become chat
cards.

The history rail is derived only from user messages. Its popup is at least 50% of the
conversation column, max-width constrained, keyboard reachable, and collision-aware.

The active pane is the only target for sidebar selection and New chat. If a chosen
session is already visible, focus that pane rather than rendering the same session twice.
At compact widths, panes stack; at narrow widths, show the active pane plus a pane
switcher. A pinned right panel falls back to the per-pane popover before either composer
becomes unusable.

## 8. Source kickoff pipeline

All UI actions call one frontend request shape:

```ts
interface StartAgentSessionRequest {
  repoId: string
  source: SessionSourceInput
  intent: SessionIntent
  mode: 'ask' | 'plan' | 'auto'
  team: 'solo' | 'lead'
  providerOverride?: string
}
```

Required order:

1. Mark the clicked source as selected/starting.
2. Open or focus Agent Desk.
3. Create the durable session with cached source snapshot.
4. Select it and show Preparing.
5. Resolve provider and execution policy.
6. For Fix, provision an isolated worktree before the first edit.
7. Start the engine and append its events.
8. On any failure, keep the session and show the recovery action in it.

Do not fetch all source details in the main window before opening Agent Desk. Create from
known row data, then enrich in the Desk so slow hosts never make the click look dead.

## 9. Intent policies

Keep policy in Rust and unit test it:

| Intent | Default mode | Default team | Writes | Worktree |
| --- | --- | --- | --- | --- |
| Ask | Ask | Solo | No | No |
| Explain | Ask | Solo | No | No |
| Summarize | Ask | Solo | No | No |
| Review | Ask | Solo | No | No |
| Plan | Plan | Lead | No until Start | No until Start |
| Fix | Auto | Lead | Yes | Always isolated |

Provider/model defaults remain user settings. Source menus offer `…with` only as a
secondary override.

## 10. Agent graph

Extend the existing `RunDriver`; do not implement a second loop.

- The lead is the only agent that talks directly in the main conversation.
- A helper receives a bounded job, allowed paths, source snapshot, completion condition,
  budget, and worktree.
- Each helper has a distinct execution ID and ordered event stream.
- The graph is a projection of persisted executions and dependencies, not hand-maintained
  frontend state.
- Plan mode persists a proposed graph in `AwaitingStart`; Auto may transition it to
  Running after policy validation.
- Stop agent cancels one helper. Stop all cancels lead and helpers, preserving recoverable
  edits.
- The lead cannot mark the session finished until required helpers finish and integration
  checks complete.

Start with a tree/DAG limited to one lead plus three helpers. Reject cycles and arbitrary
nested delegation in the first release.

## 11. External adapters

Define a Rust trait whose only mandatory operation is read-only discovery:

```rust
trait AgentClientAdapter {
    fn id(&self) -> &'static str;
    fn detect(&self) -> Detection;
    fn list_sessions(&self, cursor: Option<String>) -> Result<Page<ExternalSession>>;
    fn read_session(&self, id: &str) -> Result<ExternalConversation>;
    fn read_configuration(&self) -> Result<ExternalConfiguration>;
    fn continuation(&self, id: &str) -> Option<ExternalLaunch>;
}
```

Adapters are version-gated and return `UnsupportedVersion` with detected/known versions.
Never guess a writable schema. Fixture tests use copied, anonymized session/config trees.

Implement adapters in this order: Codex, Claude Code, OpenCode, VS Code Copilot,
OpenChamber. OpenChamber may be an OpenCode-derived adapter but must retain its own
detection and version fixtures.

## 12. Usage reporting

Normalize only values the provider exposes:

- current session tokens/requests/cost;
- overall plan limits and reset time;
- active helper count; and
- data timestamp.

Every field is optional and carries `source: measured | provider_reported | estimated`.
The UI omits absent rows. It never turns unknown into zero and never invents a dollar
estimate. Usage is collapsible and its collapsed state is remembered.

## 13. Configuration sync safety

Discovery and writes are separate commands. A copy plan contains source path, destination
path, before hash, proposed content, secret references, and warnings. Apply refuses when
the destination hash changed after preview. Backup and operation receipt are written
before replacement. Undo also checks the current hash before restoring.

## 14. Logging and telemetry

Log IDs and state transitions, never message bodies, source bodies, prompts, diffs,
tokens, secrets, or file contents. Useful events:

- session_created by source/intent;
- preparation_failed by typed reason;
- execution_started/stopped/finished;
- helper_started/stopped/failed;
- import_scan outcome by adapter/version;
- configuration_copy outcome by adapter/item kind.

## 15. Test layers

- Rust unit tests: serialization migrations, atomic persistence, source enums, intent
  policy, event idempotency, graph validation, adapter fixtures, config merge safety.
- Rust integration tests: create/reload session, event durability, source refresh,
  worktree per helper, cancellation, stale event rejection.
- Frontend tests: dense grouping, source banner states, mode/team combinations, history
  jump, usage omission/collapse, graph/context switching, immediate kickoff state.
- Native verification: second-window focus, no duplicate legacy window, keyboard focus,
  real host latency, provider reconnect, restart during a run, and 1000-session scrolling.
