import { useState } from 'react'
import { toast } from 'sonner'
import { useQueryClient } from '@tanstack/react-query'
import { GitFork } from 'lucide-react'
import { commands, type AgentSession, type ExecutionRecord } from '@/lib/bindings'
import { keys, unwrap } from '@/lib/queryKeys'
import { describeError, log } from '@/lib/log'
import { cn } from '@/lib/utils'

function isActiveExecution(execution: ExecutionRecord): boolean {
  return execution.state === 'working' || execution.state === 'preparing' || execution.state === 'needsInput'
}

/** Plain-language status word for one execution row. */
function executionStatusLabel(state: ExecutionRecord['state']): string {
  switch (state) {
    case 'draft':
      return 'not started'
    case 'preparing':
      return 'starting'
    case 'ready':
      return 'ready'
    case 'working':
      return 'working'
    case 'needsInput':
      return 'waiting'
    case 'finished':
      return 'done'
    case 'failed':
      return 'failed'
    case 'stopped':
      return 'stopped'
    case 'missingSource':
      return 'source missing'
  }
}

const STATUS_TONE: Record<string, string> = {
  working: 'text-accent-text',
  preparing: 'text-accent-text',
  waiting: 'text-[var(--gw-amber)]',
  failed: 'text-[var(--gw-red)]',
  stopped: 'text-muted-foreground',
  done: 'text-muted-foreground',
}

/** One row for one execution (lead or helper) in the graph list. */
function ExecutionRow({ execution, isLead }: { execution: ExecutionRecord; isLead: boolean }) {
  const status = executionStatusLabel(execution.state)
  return (
    <div
      className={cn(
        'flex items-center gap-2 rounded-md border border-border bg-panel px-2 py-1.5',
        !isLead && 'ml-3'
      )}
    >
      <span
        className={cn(
          'h-1.5 w-1.5 flex-none rounded-full',
          execution.state === 'working' || execution.state === 'preparing'
            ? 'animate-pulse bg-accent-text'
            : execution.state === 'needsInput'
              ? 'bg-[var(--gw-amber)]'
              : 'bg-muted-foreground/60'
        )}
        aria-hidden
      />
      <span className="min-w-0 flex-1">
        <strong className="block truncate text-2xs font-semibold text-foreground">
          {isLead ? 'Lead agent' : 'Helper'}
        </strong>
        <span className="block truncate text-[10px] text-muted-foreground">
          {isLead ? 'owns source + integration' : `execution ${execution.executionId.slice(0, 8)}`}
        </span>
      </span>
      <span className={cn('flex-none text-[10px] font-semibold', STATUS_TONE[status] ?? 'text-muted-foreground')}>
        {status}
      </span>
    </div>
  )
}

/**
 * The Graph tab (tasks.md 7.1, 7.5) and its "Stop all" header control
 * (tasks.md 6.5): the ONLY stop control in the composer/graph surface, per
 * the mockup's `.ag-stop-all` -- there is deliberately no icon-only stop
 * button beside the composer's Send.
 *
 * Rows are built directly from `session.executions` (`ExecutionRecord`,
 * `src/lib/bindings.ts`) rather than the mockup's richer per-node copy
 * ("Trace the crash", file counts, etc.) -- `ExecutionRecord` does not carry
 * a job title, description, or file-change summary today, so inventing that
 * text would misrepresent what the backend actually reports. What IS shown
 * (state, whether it is the lead vs. a helper, and its ID) is exactly what
 * `AgentSession.executions` provides.
 */
export function AgentGraphPanel({ session }: { session: AgentSession }) {
  const qc = useQueryClient()
  const [stopping, setStopping] = useState(false)

  const executions = session.executions
  const lead = executions.find((e) => e.parentExecutionId === null) ?? null
  const helpers = executions.filter((e) => e.parentExecutionId !== null)
  const activeCount = executions.filter(isActiveExecution).length
  const waitingCount = executions.filter((e) => e.state === 'needsInput').length

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

  return (
    <div className="flex h-full flex-col gap-2">
      <div className="flex flex-none items-center gap-2 pb-1">
        <span className="min-w-0 flex-1 truncate text-2xs text-muted-foreground">
          {activeCount} working · {waitingCount} waiting
        </span>
        <button
          type="button"
          onClick={() => void stopAll()}
          disabled={stopping || activeCount + waitingCount === 0}
          className="flex-none rounded border border-border bg-panel px-1.5 py-1 text-[10px] font-semibold text-foreground hover:bg-panel3 disabled:cursor-not-allowed disabled:opacity-50"
        >
          {stopping ? 'Stopping…' : 'Stop all'}
        </button>
      </div>

      <div className="flex min-h-0 flex-1 flex-col gap-1.5 overflow-y-auto">
        {lead && <ExecutionRow execution={lead} isLead />}
        {helpers.map((h) => (
          <ExecutionRow key={h.executionId} execution={h} isLead={false} />
        ))}
      </div>
    </div>
  )
}
