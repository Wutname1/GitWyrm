import { useState } from 'react'
import { toast } from 'sonner'
import { useQueryClient } from '@tanstack/react-query'
import { ListTodo, TriangleAlert } from 'lucide-react'
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
  const [busy, setBusy] = useState<'start' | 'solo' | 'accepting' | null>(null)
  // Task 3.3 ("detect task/spec changes after draft and block Start until
  // refreshed or explicitly accepted"): when Start refuses with `stale`, this
  // card shows a warning and an explicit "Start anyway" action instead of the
  // ordinary Start button. Cleared on refresh (a fresh mount re-fetches the
  // lead, which no longer carries this outcome) and on a fresh Start attempt.
  const [stale, setStale] = useState<{ currentFingerprint: string } | null>(null)
  const proposal = lead.proposedGraph
  if (!proposal) return null

  const refreshSession = () => void qc.invalidateQueries({ queryKey: keys.agentSession(session.header.sessionId) })

  const start = async () => {
    if (busy) return
    setBusy('start')
    try {
      const outcome = unwrap(await commands.agentSessionStartGraph(session.header.sessionId))
      if (outcome.kind === 'started') {
        setStale(null)
        refreshSession()
        toast.success(
          outcome.started_helpers.length > 0
            ? `Started the lead and ${outcome.started_helpers.length} helper${outcome.started_helpers.length === 1 ? '' : 's'}.`
            : 'Started the lead agent.'
        )
      } else if (outcome.kind === 'stale') {
        // Refuses outright: no worktree was provisioned, nothing was
        // written. The user must refresh (re-plan) or explicitly accept the
        // drift before Start will proceed -- see the warning banner below.
        setStale({ currentFingerprint: outcome.current_fingerprint })
        toast.warning('The OpenSpec source changed since this plan was drafted.', {
          description: 'Review what changed, then start anyway or ask for a fresh plan.',
        })
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

  /**
   * "Start anyway": records explicit acceptance of the exact current drift
   * (`agent_session_accept_stale_openspec_context` re-fingerprints from the
   * live files itself -- it never trusts a fingerprint string round-tripped
   * through the frontend), then retries Start through the ordinary `start()`
   * path so the retry gets the exact same validation and worktree/helper
   * launch every other Start does.
   */
  const startAnyway = async () => {
    if (busy) return
    setBusy('accepting')
    try {
      const outcome = unwrap(await commands.agentSessionAcceptStaleOpenspecContext(session.header.sessionId))
      if (outcome.kind !== 'accepted') {
        toast.error('Could not accept the source change.', { description: outcome.kind })
        setBusy(null)
        return
      }
      setStale(null)
      setBusy(null)
      await start()
    } catch (e) {
      const message = describeError(e)
      log.error(`agent desk: could not accept stale openspec context: ${message}`)
      toast.error('Could not accept the source change.', { description: message })
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
      {stale && (
        // R5.4/tasks.md 3.3: Start already refused once for this exact
        // drift -- this banner is why, and it stays visible (rather than a
        // one-shot toast) until the user either revises or explicitly
        // starts anyway, since a toast alone would be gone before someone
        // reads it if they stepped away from the window.
        <div className="mt-2 flex items-start gap-1.5 rounded border border-amber-600/40 bg-amber-500/10 px-2 py-1.5 text-2xs leading-relaxed text-amber-700 dark:text-amber-400">
          <TriangleAlert size={12} className="mt-px flex-none" aria-hidden />
          <span>
            The OpenSpec change changed since this plan was drafted. Revise the plan for a fresh
            read, or start anyway using this plan as drafted.
          </span>
        </div>
      )}
      <div className="mt-2 flex gap-1.5">
        {stale ? (
          <button
            type="button"
            onClick={() => void startAnyway()}
            disabled={busy !== null}
            className="rounded bg-primary px-2 py-1 text-[10px] font-semibold text-primary-foreground disabled:cursor-not-allowed disabled:opacity-50"
          >
            {busy === 'accepting' || busy === 'start' ? 'Starting…' : 'Start anyway'}
          </button>
        ) : (
          <button
            type="button"
            onClick={() => void start()}
            disabled={busy !== null}
            className="rounded bg-primary px-2 py-1 text-[10px] font-semibold text-primary-foreground disabled:cursor-not-allowed disabled:opacity-50"
          >
            {busy === 'start' ? 'Starting…' : 'Start'}
          </button>
        )}
        <button
          type="button"
          onClick={onRevise}
          disabled={busy !== null}
          className="rounded border border-border bg-panel2 px-2 py-1 text-[10px] font-semibold text-foreground hover:bg-panel3 disabled:cursor-not-allowed disabled:opacity-50"
        >
          {stale ? 'Revise plan' : 'Revise'}
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
