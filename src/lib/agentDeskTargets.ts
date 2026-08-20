import type { MessageTarget } from '@/lib/bindings'

/**
 * Maps a `MessageTarget` (tasks.md 4.4) to the destination the Agent Desk
 * window can actually reach today, plus a plain-language label for the link.
 *
 * Kept out of the component so the mapping itself -- which target kinds are
 * reachable and what they are called -- is covered by a fast `.test.ts` unit
 * test (this project's `vitest.config.ts` runs `src/**\/*.test.ts` in a Node
 * environment with no DOM, so component rendering itself is not testable
 * here; see `src/lib/agentSessionGrouping.ts` for the same pattern).
 *
 * Agent Desk is a standalone webview window (see `AgentDeskView.tsx`) with no
 * embedded diff viewer, worktree browser, or graph panel today -- those are
 * `DiffView`/`GraphView`/the OpenSpec surfaces in the *main* GitWyrm window.
 * `source` is the one target kind Agent Desk can already open, via
 * `SessionSourceBanner`'s `onOpenSource`, which `AgentDeskView.tsx` now wires
 * to `commands.agentSessionOpenSource(sessionId)` (package `agent-desk-docs`)
 * -- that command focuses the main window and emits `agent-desk://open-source`,
 * which `useAgentDeskSourceListener.ts` (mounted in `App.tsx`'s `AppInner`)
 * catches and routes to the GitHub context panel, an OpenSpec selection, or
 * the diff view depending on the session's `SessionSource` kind
 * (`src/lib/agentDeskSourceNav.ts` is the pure per-kind mapping, unit tested
 * there). Every other `MessageTarget` kind below resolves to `unavailable`
 * here rather than a link that looks live and does nothing -- see the
 * mockup's `.ag-tool-link`/`data-open-setup` pattern for what a real link
 * looks like once a destination exists, and `common-pitfalls`-style "false
 * positive" guidance against building fake affordances.
 *
 * A navigation bridge into the main window also EXISTS for one more
 * destination: `commands.agentResultOpenDiff(worktreePath, path)` (agent-
 * desk-review-and-landing tasks.md 2.2) focuses the main window and opens a
 * result's worktree diff there (`src/hooks/useAgentResultDiff.ts`'s
 * `agent-result://open-diff` listener, wired at `App.tsx`'s `AppInner`).
 * The `diff` case below stays `unavailable` because `MessageTarget::Diff`'s
 * `scope` field is not yet populated with a worktree path anywhere in the
 * backend (only test fixtures construct one, per
 * `src-tauri/src/agentdesk/model.rs`) -- there is nothing to resolve
 * `target.scope` INTO yet, not a missing destination. Once a real caller
 * attaches a `Diff` target carrying a worktree path (or the review panel
 * calls `agentResultOpenDiff` directly rather than through a message
 * target), route it through that same command instead of adding a second
 * bridge. `graphNode` becomes reachable once section 7's Graph panel exists
 * in this window; `file`/`openSpecTask` still need their own bridges (a
 * *message-target* `openSpecTask`, i.e. a chat reply linking to a specific
 * task -- distinct from a *session source* `openSpecTask`, which
 * `agentDeskSourceNav.ts` already routes to its parent change).
 */
export type ResolvedMessageTarget =
  | { kind: 'source'; label: string }
  | { kind: 'unavailable'; label: string; reason: string }

export function resolveMessageTarget(target: MessageTarget): ResolvedMessageTarget {
  switch (target.kind) {
    case 'source':
      return { kind: 'source', label: 'Open the source' }
    case 'file':
      return {
        kind: 'unavailable',
        label: target.path,
        reason: 'Opening files from a chat is not available in this window yet.',
      }
    case 'diff':
      return {
        kind: 'unavailable',
        label: target.scope || 'View diff',
        reason: 'The diff viewer is not available in this window yet.',
      }
    case 'graphNode':
      return {
        kind: 'unavailable',
        label: 'View in graph',
        reason: 'Open the Graph tab to see this agent’s work.',
      }
    case 'openSpecTask':
      return {
        kind: 'unavailable',
        label: `Task ${target.taskIndex + 1}`,
        reason: 'Opening a specific task from a chat is not available yet.',
      }
  }
}
