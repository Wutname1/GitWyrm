import { useState } from 'react'
import { toast } from 'sonner'
import { useQueryClient } from '@tanstack/react-query'
import { ListTodo } from 'lucide-react'
import { commands, type AgentSession, type ExecutionRecord } from '@/lib/bindings'
import { keys, unwrap } from '@/lib/queryKeys'
import { describeError, log } from '@/lib/log'

/** Plain-language reason a proposed graph failed to validate, matching
 * `GraphValidationError`'s cases without exposing internal field names. */
function invalidGraphReason(reason: string): string {
  switch (reason) {
    case 'tooManyHelpers':
      return 'This plan has more than 3 helpers, which is not allowed yet.'
    case 'duplicateNodeId':
      return 'Two helpers in this plan have the same id.'
    case 'unknownDependency':
      return "One helper depends on a step that isn't in this plan."
    case 'cycle':
      return 'Two or more helpers depend on each other in a loop.'
    case 'emptyJob':
      return 'One helper is missing a title or a real time budget.'
    case 'missingAllowedPaths':
      return "One helper can write files but wasn't told which ones it may touch."
    default:
      return 'This plan is no longer valid.'
  }
}

/**
 * Plan mode's AwaitingStart card (tasks.md 2.2): shown when a lead execution
 * carries a `proposedGraph` and has not been started yet. Offers Start,
 * Revise, and "Use solo instead" -- every action changes graph state
 * immediately and visibly (tasks.md 2.4), so each branch invalidates the
 * session query on success rather than waiting for a later refetch.
 */
export function AwaitingStartCard({
  session,
  lead,
  onRevise,
}: {
  session: AgentSession
  lead: ExecutionRecord
  onRevise: () => void
}) {
  const qc = useQueryClient()
  const [busy, setBusy] = useState<'start' | 'solo' | null>(null)
  const proposal = lead.proposedGraph
  if (!proposal) return null

  const refreshSession = () => void qc.invalidateQueries({ queryKey: keys.agentSession(session.header.sessionId) })

  const start = async () => {
    if (busy) return
    setBusy('start')
    try {
      const outcome = unwrap(await commands.agentSessionStartGraph(session.header.sessionId))
      if (outcome.kind === 'started') {
        refreshSession()
        toast.success(
          outcome.started_helpers.length > 0
            ? `Started the lead and ${outcome.started_helpers.length} helper${outcome.started_helpers.length === 1 ? '' : 's'}.`
            : 'Started the lead agent.'
        )
      } else if (outcome.kind === 'noProposal') {
        toast.error('There is no plan waiting to start.')
      } else if (outcome.kind === 'invalid') {
        toast.error('This plan can no longer start.', { description: invalidGraphReason(outcome.reason.kind) })
      } else if (outcome.kind === 'worktreeFailed') {
        toast.error(`Could not set up a workspace for "${outcome.node_id}".`, { description: outcome.detail })
      } else if (outcome.kind === 'sourceMissing') {
        toast.error('This chat needs its repository open to start.', { description: outcome.detail })
      } else if (outcome.kind === 'notFound') {
        toast.error('This chat is gone. It may have been archived elsewhere.')
      } else if (outcome.kind === 'damaged') {
        toast.error('This chat file is damaged and could not start.', { description: outcome.reason })
      } else {
        toast.error('Could not start the plan.', { description: outcome.kind })
      }
    } catch (e) {
      const message = describeError(e)
      log.error(`agent desk: could not start graph: ${message}`)
      toast.error('Could not start the plan.', { description: message })
    } finally {
      setBusy(null)
    }
  }

  const useSolo = async () => {
    if (busy) return
    setBusy('solo')
    try {
      const outcome = unwrap(await commands.agentSessionUseSoloInstead(session.header.sessionId))
      if (outcome.kind === 'cleared') {
        refreshSession()
        toast.success('Switched to a single agent for this chat.')
      } else if (outcome.kind === 'noProposal') {
        toast.error('There is no plan waiting to start.')
      } else if (outcome.kind === 'notFound') {
        toast.error('This chat is gone. It may have been archived elsewhere.')
      } else if (outcome.kind === 'damaged') {
        toast.error('This chat file is damaged.', { description: outcome.reason })
      } else {
        toast.error('Could not switch to a single agent.', { description: outcome.kind })
      }
    } catch (e) {
      const message = describeError(e)
      log.error(`agent desk: could not use solo instead: ${message}`)
      toast.error('Could not switch to a single agent.', { description: message })
    } finally {
      setBusy(null)
    }
  }

  return (
    <div className="flex-none rounded-md border border-border bg-panel p-2.5">
      <div className="flex items-center gap-1.5 text-[9px] font-bold uppercase tracking-wide text-muted-foreground">
        <ListTodo size={11} aria-hidden />
        Plan ready to review
      </div>
      <p className="mt-1.5 text-2xs leading-relaxed text-foreground">{proposal.leadSummary}</p>
      <ul className="mt-1.5 flex flex-col gap-1">
        {proposal.helpers.map((h) => (
          <li key={h.nodeId} className="rounded border border-border bg-panel2 px-1.5 py-1 text-[10px]">
            <span className="font-semibold text-foreground">{h.title}</span>
            <span className="text-muted-foreground"> · {h.role}</span>
          </li>
        ))}
      </ul>
      <div className="mt-2 flex gap-1.5">
        <button
          type="button"
          onClick={() => void start()}
          disabled={busy !== null}
          className="rounded bg-primary px-2 py-1 text-[10px] font-semibold text-primary-foreground disabled:cursor-not-allowed disabled:opacity-50"
        >
          {busy === 'start' ? 'Starting…' : 'Start'}
        </button>
        <button
          type="button"
          onClick={onRevise}
          disabled={busy !== null}
          className="rounded border border-border bg-panel2 px-2 py-1 text-[10px] font-semibold text-foreground hover:bg-panel3 disabled:cursor-not-allowed disabled:opacity-50"
        >
          Revise
        </button>
        <button
          type="button"
          onClick={() => void useSolo()}
          disabled={busy !== null}
          className="rounded border border-border bg-panel2 px-2 py-1 text-[10px] font-semibold text-foreground hover:bg-panel3 disabled:cursor-not-allowed disabled:opacity-50"
        >
          {busy === 'solo' ? 'Switching…' : 'Use solo instead'}
        </button>
      </div>
    </div>
  )
}
