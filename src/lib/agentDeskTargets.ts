import type { ExecutionRecord, MessageTarget } from '@/lib/bindings'

/**
 * Maps a `MessageTarget` (tasks.md 4.4) to the destination the Agent Desk
 * window can reach, plus a plain-language label for the link.
 *
 * Kept out of the component so the mapping itself, which target kinds are
 * reachable and what they are called, is covered by a fast `.test.ts` unit
 * test (this project's `vitest.config.ts` runs `src/**\/*.test.ts` in a Node
 * environment with no DOM, so component rendering itself is not testable
 * here; see `src/lib/agentSessionGrouping.ts` for the same pattern). The
 * resolver is pure: it decides WHAT to do and returns a description; the
 * clicking is done by `src/hooks/useMessageTargetNav.ts`, which owns the
 * commands, the store writes and the toasts.
 *
 * Every kind rides on navigation that already exists rather than a bridge
 * built for this file:
 *   - `source` -> `SessionSourceBanner`'s `onOpenSource`, which
 *     `AgentDeskView.tsx` wires to `commands.agentSessionOpenSource` (the
 *     main window then routes via `agentDeskSourceNav.ts`).
 *   - `file`/`diff` -> `commands.agentResultOpenDiff(worktreePath, path)`,
 *     the same bridge the review panel and the graph inspector use: the
 *     main window opens `worktreePath` as a repo tab and shows the
 *     working-tree diff for `path` (`useAgentResultDiff.ts`). A message from
 *     a helper that runs in its own worktree opens THAT worktree; anything
 *     else (the lead, a read-only helper, a note with no execution) falls
 *     back to the session's own repository, so the link still lands on the
 *     file the message is talking about.
 *   - `graphNode` -> the Agent graph dock in this window
 *     (`agentDeskUiStore.selectGraphNode` + `openDock('graph')`).
 *   - `openSpecTask` -> this window's own Spec view
 *     (`OpenSpecEmbeddedDetail`, via `selectChangeEverywhere` and
 *     `agentDeskUiStore.setCenterView('openspec')`). There is no per-task
 *     selection surface anywhere in the app (only per-change), so the link
 *     honestly lands on the parent change and the toast says which task to
 *     look for, matching `agentDeskSourceNav.ts`'s stance for task sources.
 *
 * `unavailable` is reserved for a target the app genuinely cannot show
 * right now, and always carries a reason the UI can put in front of the
 * user: a chat with no project has nowhere to open a file; a graph node for
 * an agent the chat no longer lists, or for a chat with no team, has no
 * panel that could highlight it.
 */
export type ResolvedMessageTarget =
  | { kind: 'source'; label: string }
  | {
      kind: 'diff'
      label: string
      /** Absolute path the main window opens as a repo tab. */
      worktreePath: string
      /** Repo-relative file to show, or null to just bring that repo forward. */
      path: string | null
      /** Whether `worktreePath` is a helper's isolated worktree or the chat's own repository. */
      location: 'helperWorktree' | 'repo'
    }
  | { kind: 'graphNode'; label: string; executionId: string }
  | { kind: 'openSpecTask'; label: string; changeId: string; taskIndex: number }
  | { kind: 'unavailable'; label: string; reason: string }

export interface MessageTargetContext {
  /** Absolute path of the chat's repository, or null when the chat has no project yet. */
  repoPath: string | null
  /** The chat's executions, for worktree lookup and graph membership. */
  executions: ReadonlyArray<Pick<ExecutionRecord, 'executionId' | 'parentExecutionId' | 'worktreePath'>>
  /** The execution the message came from, so a helper's file opens in that helper's worktree. */
  messageExecutionId: string | null
  /** Whether the chat has a team graph to show at all (`sessionHasGraph`). */
  hasGraph: boolean
}

/** Characters that make a diff scope a pattern ("src/**") rather than one file the diff viewer can open. */
const GLOB_CHARS = /[*?[\]{}]/

/**
 * A `diff` target's `scope` is free text from the backend: sometimes a file,
 * sometimes a glob or a description. Only a concrete path can be handed to
 * the diff viewer; anything else opens the repo without a file selected.
 */
export function diffScopePath(scope: string): string | null {
  const trimmed = scope.trim()
  if (trimmed.length === 0 || GLOB_CHARS.test(trimmed)) return null
  return trimmed
}

/** Where a message's file should open: the sending helper's worktree if it has one, else the chat's repo. */
function worktreeFor(ctx: MessageTargetContext): { worktreePath: string; location: 'helperWorktree' | 'repo' } | null {
  if (ctx.messageExecutionId) {
    const execution = ctx.executions.find((e) => e.executionId === ctx.messageExecutionId)
    if (execution?.worktreePath) {
      return { worktreePath: execution.worktreePath, location: 'helperWorktree' }
    }
  }
  if (ctx.repoPath) return { worktreePath: ctx.repoPath, location: 'repo' }
  return null
}

const NO_PROJECT_REASON = 'This chat is not linked to a project yet, so there is no file to open.'

export function resolveMessageTarget(target: MessageTarget, ctx: MessageTargetContext): ResolvedMessageTarget {
  switch (target.kind) {
    case 'source':
      return { kind: 'source', label: 'Open the source' }
    case 'file': {
      const where = worktreeFor(ctx)
      if (!where) return { kind: 'unavailable', label: target.path, reason: NO_PROJECT_REASON }
      return { kind: 'diff', label: target.path, path: target.path, ...where }
    }
    case 'diff': {
      const where = worktreeFor(ctx)
      const label = target.scope.trim() || 'View diff'
      if (!where) return { kind: 'unavailable', label, reason: NO_PROJECT_REASON }
      return { kind: 'diff', label, path: diffScopePath(target.scope), ...where }
    }
    case 'graphNode': {
      const label = 'View in graph'
      if (!ctx.executions.some((e) => e.executionId === target.executionId)) {
        return { kind: 'unavailable', label, reason: 'That agent is no longer part of this chat.' }
      }
      if (!ctx.hasGraph) {
        return {
          kind: 'unavailable',
          label,
          reason: 'This chat is one agent working alone, so there is no team graph to show.',
        }
      }
      return { kind: 'graphNode', label, executionId: target.executionId }
    }
    case 'openSpecTask':
      return {
        kind: 'openSpecTask',
        label: `Task ${target.taskIndex + 1}`,
        changeId: target.changeId,
        taskIndex: target.taskIndex,
      }
  }
}
