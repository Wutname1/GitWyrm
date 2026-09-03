import { useCallback, useMemo } from 'react'
import { useQueryClient } from '@tanstack/react-query'
import { toast } from 'sonner'
import { commands, type AgentSession, type MessageTarget } from '@/lib/bindings'
import { keys, unwrap } from '@/lib/queryKeys'
import { describeError, log } from '@/lib/log'
import { resolveMessageTarget, type ResolvedMessageTarget } from '@/lib/agentDeskTargets'
import { sessionHasGraph } from '@/lib/agentDeskGraph'
import { isRightDockSafeAtWidth } from '@/lib/agentDeskDockPlacement'
import { selectChangeEverywhere } from '@/lib/specSync'
import { useAgentDeskUiStore } from '@/stores/agentDeskUiStore'

/**
 * The clicking half of message-target links (tasks.md 4.4). `resolveMessageTarget`
 * decides what a link means for this chat; this hook carries it out and
 * gives the click a visible answer every time (Rule #1): a window comes
 * forward, a panel opens, or a toast says why nothing could.
 *
 * `source` is deliberately not handled here: it already goes through
 * `AgentDeskView`'s `openSourceFor` (the `onOpenSource` prop), and the
 * banner and every source link share that one path.
 */
export interface MessageTargetNav {
  resolve: (target: MessageTarget, messageExecutionId: string | null) => ResolvedMessageTarget
  open: (resolved: ResolvedMessageTarget) => void
}

export function useMessageTargetNav(session: AgentSession | null): MessageTargetNav {
  const qc = useQueryClient()
  const layoutDock = useAgentDeskUiStore((s) => s.layout.dock)
  const openDock = useAgentDeskUiStore((s) => s.openDock)
  const selectGraphNode = useAgentDeskUiStore((s) => s.selectGraphNode)
  const setCenterView = useAgentDeskUiStore((s) => s.setCenterView)

  const sessionId = session?.header.sessionId ?? null
  const repoId = session?.header.repoId ?? null
  const repoPath = session?.header.repoPath ?? null
  const executions = session?.executions
  const hasGraph = sessionHasGraph(session)

  const resolve = useCallback(
    (target: MessageTarget, messageExecutionId: string | null) =>
      resolveMessageTarget(target, {
        repoPath,
        executions: executions ?? [],
        messageExecutionId,
        hasGraph,
      }),
    [repoPath, executions, hasGraph]
  )

  const open = useCallback(
    (resolved: ResolvedMessageTarget) => {
      switch (resolved.kind) {
        case 'source':
          // Owned by `onOpenSource`; see the hook comment.
          return
        case 'unavailable':
          toast.info(resolved.reason)
          return
        case 'diff':
          void openDiffInMainWindow(resolved)
          return
        case 'graphNode': {
          if (!sessionId) return
          selectGraphNode(sessionId, resolved.executionId)
          if (layoutDock?.kind !== 'graph') {
            // Keep the edge the user already chose for the dock. With no dock
            // yet, a right dock on a narrow window renders as popover-only
            // (`resolveDockVisibility`), which would make this click look
            // ignored, so fall to the bottom edge there.
            const edge = layoutDock?.edge ?? (isRightDockSafeAtWidth(window.innerWidth) ? 'right' : 'bottom')
            openDock('graph', edge, layoutDock?.leftOrder)
          }
          toast.success('Showing that agent in the graph panel.')
          return
        }
        case 'openSpecTask':
          void openSpecTask(resolved)
          return
      }
    },
    [sessionId, layoutDock, openDock, selectGraphNode]
  )

  async function openDiffInMainWindow(resolved: Extract<ResolvedMessageTarget, { kind: 'diff' }>) {
    try {
      const outcome = unwrap(await commands.agentResultOpenDiff(resolved.worktreePath, resolved.path))
      if (outcome.kind === 'mainWindowNotOpen') {
        toast.error('Open the main GitWyrm window first.')
        return
      }
      const where = resolved.location === 'helperWorktree' ? "that helper's changes" : 'the project'
      toast.success(
        resolved.path
          ? `Showing ${resolved.path} in the main GitWyrm window.`
          : `Showing ${where} in the main GitWyrm window.`
      )
    } catch (e) {
      const message = describeError(e)
      log.error(`agent desk: could not open ${resolved.path ?? resolved.worktreePath}: ${message}`)
      toast.error('Could not open that file.', { description: message })
    }
  }

  async function openSpecTask(resolved: Extract<ResolvedMessageTarget, { kind: 'openSpecTask' }>) {
    if (!repoId) {
      toast.info('This chat is not linked to a project yet, so there is no change to open.')
      return
    }
    try {
      // The change may have been archived since the message was written. Ask
      // before switching views so the Spec view never opens on nothing.
      const changes = await qc.fetchQuery({
        queryKey: keys.openspecChanges(repoId),
        queryFn: async () => unwrap(await commands.openspecListChanges(repoId)),
      })
      if (!changes.some((c) => c.id === resolved.changeId)) {
        toast.error(`Change "${resolved.changeId}" is not in this project any more. It may have been archived.`)
        return
      }
      selectChangeEverywhere(resolved.changeId)
      setCenterView('openspec')
      toast.success(`Showing change "${resolved.changeId}". Look for task ${resolved.taskIndex + 1} in its task list.`)
    } catch (e) {
      const message = describeError(e)
      log.error(`agent desk: could not open change ${resolved.changeId}: ${message}`)
      toast.error('Could not open that change.', { description: message })
    }
  }

  return useMemo(() => ({ resolve, open }), [resolve, open])
}
