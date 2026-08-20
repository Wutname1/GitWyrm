import { useEffect, useState } from 'react'
import { toast } from 'sonner'
import { useQueryClient } from '@tanstack/react-query'
import { GitFork } from 'lucide-react'
import { commands, type AgentSession, type ExecutionRecord } from '@/lib/bindings'
import { keys, unwrap } from '@/lib/queryKeys'
import { describeError, log } from '@/lib/log'
import { cn } from '@/lib/utils'
import { buildGraphTree, graphSummary, nodeDotTone, nodeStatusLabel, type GraphTreeNode } from '@/lib/agentGraphProjection'
import { AwaitingStartCard } from './AwaitingStartCard'

const STATUS_TONE: Record<string, string> = {
  working: 'text-accent-text',
  starting: 'text-accent-text',
  waiting: 'text-[var(--gw-amber)]',
  conflict: 'text-[var(--gw-amber)]',
  failed: 'text-[var(--gw-red)]',
  stopped: 'text-muted-foreground',
  done: 'text-added',
  queued: 'text-muted-foreground',
}

const DOT_TONE: Record<string, string> = {
  lead: 'bg-added',
  done: 'bg-added',
  working: 'bg-accent-text animate-pulse',
  waiting: 'bg-[var(--gw-amber)]',
}

/** One row in the graph tree: an L-shaped connector for helper rows, a
 * status dot, title + meta, and the status word (mockup `.ag-node-wrap`,
 * `.ag-node-dot`, `.ag-node-status`). */
function GraphNodeRow({
  node,
  selected,
  onSelect,
}: {
  node: GraphTreeNode
  selected: boolean
  onSelect: () => void
}) {
  const { execution, isLead } = node
  const status = nodeStatusLabel(node)
  const tone = nodeDotTone(node)
  const title = isLead ? 'Lead agent' : (execution.jobTitle ?? 'Helper')
  const meta = isLead
    ? 'owns source + integration'
    : [execution.helperRole, execution.worktreePath ? 'worktree' : 'read-only'].filter(Boolean).join(' · ')

  return (
    <div className={cn('relative', !isLead && 'pl-6')}>
      {!isLead && (
        <span
          aria-hidden
          className="pointer-events-none absolute left-2.5 top-0 h-1/2 w-3 border-b border-l border-border"
        />
      )}
      <button
        type="button"
        onClick={onSelect}
        className={cn(
          'flex w-full items-center gap-2 rounded-md border px-2 py-1.5 text-left transition-colors',
          selected ? 'border-accent bg-panel3' : 'border-border bg-panel hover:bg-panel3'
        )}
      >
        <span
          className={cn(
            'h-2.5 w-2.5 flex-none rounded-full',
            tone ? DOT_TONE[tone] : 'bg-muted-foreground/50',
            isLead && 'rounded-[3px_6px_3px_6px]'
          )}
          aria-hidden
        />
        <span className="min-w-0 flex-1">
          <strong className="block truncate text-2xs font-semibold text-foreground">{title}</strong>
          <span className="block truncate text-[10px] text-muted-foreground">{meta || 'agent'}</span>
        </span>
        <span className={cn('flex-none text-[10px] font-semibold', STATUS_TONE[status] ?? 'text-muted-foreground')}>
          {status}
        </span>
      </button>
    </div>
  )
}

/** The persistent inspector card (mockup `.ag-inspector`): kicker, title,
 * description, files line, and Open conversation / Stop agent actions. */
function InspectorCard({
  node,
  session,
  onStopped,
}: {
  node: GraphTreeNode
  session: AgentSession
  onStopped: () => void
}) {
  const qc = useQueryClient()
  const [stopping, setStopping] = useState(false)
  const { execution, isLead } = node
  const title = isLead ? 'Lead agent' : (execution.jobTitle ?? 'Helper')
  const description = isLead
    ? 'Owns the conversation, the source, and reviews every helper result before answering.'
    : (execution.jobDescription ?? 'No description yet.')
  const changedFileCount = execution.changedFileCount ?? 0
  const filesLine = isLead
    ? undefined
    : [
        execution.helperRole,
        changedFileCount > 0 ? `${changedFileCount} file${changedFileCount === 1 ? '' : 's'} changed` : undefined,
      ]
        .filter(Boolean)
        .join(' · ')
  const canStop = execution.state === 'working' || execution.state === 'preparing' || execution.state === 'needsInput'

  const stopThis = async () => {
    if (stopping) return
    setStopping(true)
    try {
      const outcome = unwrap(
        await commands.agentSessionStopExecution(session.header.sessionId, {
          kind: 'one',
          execution_id: execution.executionId,
        })
      )
      if (outcome.kind === 'stopped') {
        void qc.invalidateQueries({ queryKey: keys.agentSession(session.header.sessionId) })
        toast.success(
          outcome.stopped.length > 0
            ? isLead
              ? 'Lead agent stopped; work already done was kept.'
              : 'Agent stopped; other work continues.'
            : 'That agent already finished.'
        )
        onStopped()
      } else if (outcome.kind === 'notFound') {
        toast.error('This chat is gone. It may have been archived elsewhere.')
      } else if (outcome.kind === 'damaged') {
        toast.error('This chat file is damaged and could not be stopped.', { description: outcome.reason })
      } else {
        toast.error('Could not stop that agent.', { description: outcome.kind })
      }
    } catch (e) {
      const message = describeError(e)
      log.error(`agent desk: could not stop execution ${execution.executionId}: ${message}`)
      toast.error('Could not stop that agent.', { description: message })
    } finally {
      setStopping(false)
    }
  }

  return (
    <div className="flex-none rounded-md border border-border bg-panel p-2.5">
      <div className="text-[9px] font-bold uppercase tracking-wide text-muted-foreground">Selected agent</div>
      <div className="mt-1 text-xs font-semibold text-foreground">{title}</div>
      <p className="mt-1 text-2xs leading-relaxed text-muted-foreground">{description}</p>
      {filesLine ? <div className="mt-1.5 font-mono text-[10px] text-muted-foreground">{filesLine}</div> : null}
      <div className="mt-2 flex gap-1.5">
        <button
          type="button"
          disabled
          title="A helper's own conversation cannot be opened yet"
          className="rounded border border-border bg-panel2 px-1.5 py-1 text-[10px] font-semibold text-foreground opacity-40"
        >
          Open conversation
        </button>
        <button
          type="button"
          onClick={() => void stopThis()}
          disabled={!canStop || stopping}
          className="rounded border border-border bg-panel2 px-1.5 py-1 text-[10px] font-semibold text-foreground hover:bg-panel3 disabled:cursor-not-allowed disabled:opacity-50"
        >
          {stopping ? 'Stopping…' : 'Stop agent'}
        </button>
      </div>
    </div>
  )
}

/**
 * The Graph tab (tasks.md 6.1, 6.2, 7.1, 7.5): an indented tree of the
 * session's lead and helper executions, a persistent inspector for whichever
 * node is selected, and a labeled Stop-all header control (tasks.md 4.2) --
 * the ONLY stop control in the composer/graph surface besides the
 * inspector's own per-node Stop agent (tasks.md 4.1).
 *
 * Every row is a direct projection of `session.executions`
 * (`ExecutionRecord`, `src/lib/bindings.ts`) via `buildGraphTree`
 * (`src/lib/agentGraphProjection.ts`) -- there is no separate frontend graph
 * state to go stale (tasks.md 6.1).
 */
export function AgentGraphPanel({ session }: { session: AgentSession }) {
  const qc = useQueryClient()
  const [stopping, setStopping] = useState(false)
  const [selectedId, setSelectedId] = useState<string | null>(null)

  const executions = session.executions
  const tree = buildGraphTree(executions)
  const activeCount = executions.filter(
    (e) => e.state === 'working' || e.state === 'preparing' || e.state === 'needsInput'
  ).length

  useEffect(() => {
    // Keep a selection alive across re-renders: default to the lead, and
    // fall back to the lead again if the selected node disappears (e.g. a
    // stopped helper eventually pruned from a later session read).
    if (selectedId && executions.some((e) => e.executionId === selectedId)) return
    const lead = executions.find((e) => e.parentExecutionId === null)
    setSelectedId(lead?.executionId ?? executions[0]?.executionId ?? null)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [session.header.sessionId, executions.length])

  const stopAll = async () => {
    if (stopping) return
    setStopping(true)
    try {
      const outcome = unwrap(await commands.agentSessionStopExecution(session.header.sessionId, { kind: 'all' }))
      if (outcome.kind === 'stopped') {
        void qc.invalidateQueries({ queryKey: keys.agentSession(session.header.sessionId) })
        toast.success(
          outcome.stopped.length > 0
            ? `Stopped ${outcome.stopped.length} agent${outcome.stopped.length === 1 ? '' : 's'}; work already done was kept.`
            : 'Nothing was running.'
        )
      } else if (outcome.kind === 'notFound') {
        toast.error('This chat is gone. It may have been archived elsewhere.')
      } else if (outcome.kind === 'damaged') {
        toast.error('This chat file is damaged and could not be stopped.', { description: outcome.reason })
      } else {
        toast.error('Could not stop the agents.', { description: outcome.kind })
      }
    } catch (e) {
      const message = describeError(e)
      log.error(`agent desk: could not stop all executions: ${message}`)
      toast.error('Could not stop the agents.', { description: message })
    } finally {
      setStopping(false)
    }
  }

  if (executions.length === 0) {
    // tasks.md 7.5: explain Solo/Plan/Auto in plain language rather than
    // showing a bare "nothing here" -- verbatim mockup copy.
    return (
      <div className="flex h-full flex-col items-center justify-center gap-2 px-4 py-8 text-center">
        <GitFork size={26} className="text-muted-foreground" aria-hidden />
        <p className="text-xs font-semibold text-foreground">No graph in a solo chat.</p>
        <p className="max-w-[16rem] text-2xs leading-relaxed text-muted-foreground">
          Choose Lead + helpers below the composer. In Plan, review the graph before it starts. In Auto, Sol starts
          helpers when the work splits safely.
        </p>
      </div>
    )
  }

  const selected = tree.find((n) => n.execution.executionId === selectedId) ?? tree[0]
  const awaitingStartLead = executions.find((e) => e.parentExecutionId === null && e.proposedGraph !== null)

  if (awaitingStartLead) {
    // tasks.md 2.2: Plan mode waits here until Start/Revise/Use-solo -- no
    // tree to show yet, since nothing has an execution beyond the lead
    // itself.
    return (
      <div className="flex h-full flex-col gap-2">
        <AwaitingStartCard
          session={session}
          lead={awaitingStartLead}
          onRevise={() => toast('Revise the plan in the chat, then send it again.')}
        />
      </div>
    )
  }

  return (
    <div className="flex h-full flex-col gap-2">
      <div className="flex flex-none items-center gap-2 pb-1">
        <span className="min-w-0 flex-1 truncate text-2xs text-muted-foreground">{graphSummary(executions)}</span>
        <button
          type="button"
          onClick={() => void stopAll()}
          disabled={stopping || activeCount === 0}
          className="flex-none rounded border border-border bg-panel px-1.5 py-1 text-[10px] font-semibold text-foreground hover:bg-panel3 disabled:cursor-not-allowed disabled:opacity-50"
        >
          {stopping ? 'Stopping…' : 'Stop all'}
        </button>
      </div>

      <div className="flex min-h-0 flex-1 flex-col gap-1.5 overflow-y-auto">
        {tree.map((node) => (
          <GraphNodeRow
            key={node.execution.executionId}
            node={node}
            selected={node.execution.executionId === selected?.execution.executionId}
            onSelect={() => setSelectedId(node.execution.executionId)}
          />
        ))}
      </div>

      {selected ? (
        <InspectorCard session={session} node={selected} onStopped={() => void qc.invalidateQueries({ queryKey: keys.agentSession(session.header.sessionId) })} />
      ) : null}
    </div>
  )
}
