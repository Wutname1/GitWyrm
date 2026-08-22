# Overnight run state - 2026-08-20

## Committed and verified

| commit | what |
|---|---|
| `9c4299c` | add-ai-agent-engine spec rewritten to the CLI-only reality; dead ApiKey transport removed |
| `f6bb5d2` | session foundation: durable chats, atomic writes, index rebuild, per-session locks, run bridge |
| `5a1d2d8` | one app-wide Agent Desk window, real chat list, execution commands |
| `57fc273` | window retarget listener + start/stop execution race fixes (both reviewers found the listener) |
| `3c95240` | transcript, history rail, composer, context/graph panels |
| `ae75962` | child window can be dragged and maximized on its own |

Verified at `ae75962`: 749 Rust tests, 464 JS tests, typecheck clean.

## In flight, NOT yet committed

Eight agents were launched to finish the remaining 226 tasks. Landed on disk but
uncommitted at the time of writing:

- `src-tauri/src/agentdesk/policy.rs` - intent policy. Read-only is a data property
  (`can_write: false`, `WorktreePolicy::Never`), not a prompt instruction, so Review and
  Summarize can never write regardless of mode or team.
- `src-tauri/src/agentdesk/openspec_context.rs`, `adapters/`, `src-tauri/src/agent_config/`
- `ExecutionRecord` extended with `job_title`, `job_description`, `helper_role` and more -
  this closes the model gap that made the graph panel render "Helper" plus a hex id.
- Transcript blocks: `ThoughtBlock`, `PlanChecklist`, `EventStack` plus
  `agentDeskPlan/Events/Transcript.ts` (530 JS tests passing).
- Dock and pane libs: `agentDeskDock*.ts`, `agentDeskPaneTargeting.ts`,
  `useAgentDeskPanelDrag.ts`, `agentDeskDragStore.ts`.

## Critical path to a green tree

One file blocks everything: **`src-tauri/src/commands/agent_result.rs`** (4 compile
errors, its agent still running). Nothing else can proceed past it, because
`export_bindings` is a Rust binary - it cannot run while the crate does not compile.

Order of operations once every agent has reported:

1. Fix the 4 errors in `agent_result.rs`. All four are ordinary Result/Option confusion
   plus one borrow escaping its function - no design change required:
   - `:82` expected `Result<&str, Error>`, found `Option<_>`
   - `:797` expected `Option<_>`, found `Result<String, Error>`
   - `:800` `?` on a `Result` inside an `Option`-returning closure (needs `.ok()?`)
   - `:174` returns a value referencing function parameter `r`
2. `cargo check` until clean, then:
   `cargo run --manifest-path src-tauri/Cargo.toml --bin export_bindings` from the repo
   root. Never hand-edit `src/lib/bindings.ts`.
3. That clears most TypeScript errors at once. The kickoff commands already exist and are
   registered (`lib.rs:328-329` -> `agent_session_start`, `agent_intent_policy`); the
   frontend is simply calling commands whose bindings have not been emitted yet.
4. Re-run `cargo test --lib`, `npm run typecheck`, `npm run test:unit`. Only then commit.

## Where things stood at the last check

- JS tests: **553 passing across 48 files** (baseline 464).
- workspace-layout: **60/71** ticked. The orphaned store is fully wired -
  `AgentDeskView.tsx` and `SessionComposer.tsx` both consume it, and composer drafts are
  session-keyed rather than component-local, so switching chats keeps your text.
- Rust: 4 errors, all in `agent_result.rs`. The earlier `ExecutionRecord` collisions are
  resolved; the 9 new fields are threaded through every construction site.

## Landed on disk (uncommitted)

Rust: `agentdesk/{policy,graph,result,reconcile,import_store,openspec_context}.rs`,
`agentdesk/adapters/`, `agent_config/`, and commands
`agent_{config,graph,import,kickoff,result}.rs`.

Frontend: the dock and pane system (`AgentDeskDockDropZones`, `DockedDetailPanel`,
`PaneDetailPopover`, `AgentWorkspaceToolbar`, `useAgentDeskPanelDrag`,
`agentDeskDock*.ts`, `useContainerWidth`), transcript blocks (`ThoughtBlock`,
`PlanChecklist`, `EventStack`), `SessionSourcePanel`, `useStartAgentSession`,
`useOpenspecSessionSource`.

Two behaviours worth knowing about, both improvements on the mockup rather than copies
of it:
- A narrow-width **active-pane switcher**. The mockup hides content at narrow widths with
  no way back, which breaks the visible-response rule.
- `ConversationPane`'s header now survives loading, error and empty states, so a pane no
  longer loses its detail buttons mid-click.

## Still owed

- Reconcile, regenerate bindings, run the full suite, then commit.
- Competitive review per package (2 reviewers, different mandates, adversarial verify).
- The argumentative mockup-fidelity pass: one agent arguing the build betrays the
  mockup, one defending it, judged against the product vision rather than pixel-matching.
- Gate 1 and Gate 2 still need a human in the running app.
