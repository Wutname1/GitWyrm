# Agent Desk handoff

This folder is the implementation contract for evolving Spec Desk into Agent Desk. The
interactive reference is `agent-desk-mockup.html`.

## Read in this order

1. `product-brief.md` - what the product is and what it is not.
2. `implementation-reset-2026-08-21.md` - audited implementation truth, remediation work,
   and the exact order for resuming development.
3. `build-order.md` - dependency order, release slices, and stop/go checks.
4. `architecture.md` - persisted types, commands, stores, components, and boundaries.
5. `acceptance-checklist.md` - behavior that must be proven before a slice is complete.
6. The matching packages under `openspec/changes/agent-desk-*` - requirements and
   checkbox-level tasks for implementation.

## Non-negotiable interpretation

- Agent Desk replaces and expands the existing Spec Desk window. It is one app-wide
  second OS window, not one window per repository, not a third window, and not a second
  AI engine.
- The visible source answers "what started this chat?" and remains attached for the
  lifetime of the session.
- Chats are the primary object. Tasks, issues, pull requests, OpenSpec steps, commits,
  diffs, and checks are sources that can start or re-enter a chat.
- One chat fills the workspace by default. Split View shows two chats; sidebar choices
  replace the visibly active pane. Source, Context, and Graph open from each pane and can
  be pinned left, right, or bottom.
- The existing run engine, provider selection, worktrees, host providers, diff viewer,
  and OpenSpec parser are reused. Do not build parallel versions.
- A click acknowledges immediately. A slow kickoff shows the selected source, creates
  or focuses the session, and changes its state to Preparing before any provider call.
- User copy stays plain. Internal terms such as transport, turn, token budget, worktree,
  and orchestration may appear in developer diagnostics, not primary instructions.

## File ownership map

| Concern | Existing owner to extend |
| --- | --- |
| Second-window routing | `src/lib/windowMode.ts`, `src/App.tsx`, `src-tauri/src/commands/spec_desk.rs` |
| Window shell | `src/views/SpecDeskView.tsx`, `src/components/domain/spec-desk/` |
| Run state | `src/stores/aiRunStore.ts`, `src/hooks/useAiRun.ts`, `src-tauri/src/airun/` |
| Run kickoff | `src/hooks/useStartRun.ts`, `src-tauri/src/commands/airun.rs` |
| Provider selection | `src/hooks/useAiSelection.ts`, `src-tauri/src/ai/agent/` |
| Source host data | `src-tauri/src/hosting/`, `src-tauri/src/commands/github.rs` |
| Issue and PR surfaces | `src/components/domain/github/GithubContextPanel.tsx` |
| OpenSpec state | `src/hooks/useOpenspec.ts`, `src-tauri/src/openspec/` |
| Worktree isolation | `src-tauri/src/git/worktree.rs`, `src-tauri/src/commands/worktree.rs` |
| Settings | `src/stores/workspaceStore.ts`, `src/components/domain/settings/` |

## Rule for agents implementing this plan

Complete one numbered OpenSpec package at a time. Do not start a dependent package
until the package's exit gate in `build-order.md` passes. If a task requires changing a
generated binding, change the Rust type or command, run the binding exporter, and never
hand-edit `src/lib/bindings.ts`.

A task is not complete because its type, helper, command, or component exists. It is complete
only when the production entry point calls it, the result reaches the visible UI, failure and
restart behavior are handled, and the named gate evidence has been recorded.
