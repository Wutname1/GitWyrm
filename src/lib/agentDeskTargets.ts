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
 * `DiffView`/`GraphView`/the OpenSpec surfaces in the *main* GitWyrm window,
 * and no navigation bridge between the two exists yet. `source` is the one
 * target kind Agent Desk can already open, via `SessionSourceBanner`'s
 * `onOpenSource`. Every other kind resolves to `unavailable` here rather than
 * a link that looks live and does nothing -- see the mockup's
 * `.ag-tool-link`/`data-open-setup` pattern for what a real link looks like
 * once a destination exists, and `common-pitfalls`-style "false positive"
 * guidance against building fake affordances.
 *
 * `graphNode` becomes reachable once section 7's Graph panel exists in this
 * window; `file`/`diff`/`openSpecTask` become reachable once a navigation
 * bridge into the main window (or an embedded equivalent) exists. Revisit
 * this table when either lands -- it is the single place that decides
 * reachability, so nothing else has to re-derive it.
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
