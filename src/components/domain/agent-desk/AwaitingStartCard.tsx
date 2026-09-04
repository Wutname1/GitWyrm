import { useState } from 'react'
import { toast } from 'sonner'
import { useQueryClient } from '@tanstack/react-query'
import { ListTodo, TriangleAlert } from 'lucide-react'
import { commands, type AgentSession, type ExecutionRecord } from '@/lib/bindings'
import { keys, unwrap } from '@/lib/queryKeys'
import { describeError, log } from '@/lib/log'
import { describeOutcomeKind } from '@/lib/agentDeskResult'
import { useOpenRepo } from '@/hooks/useRepoActions'
import { allowedPathLines, allowedPathsLabel, helperRoleLabel } from '@/lib/agentDeskGraph'
import {
  startFailureCardForError,
  startFailureCardForGraph,
  type StartFailureAction,
  type StartFailureCard as StartFailureCardModel,
} from '@/lib/agentDeskStartFailure'
import { StartFailureCard } from './StartFailureCard'

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
  const [busy, setBusy] = useState<'start' | 'solo' | 'accepting' | 'opening' | null>(null)
  // Task 3.3 ("detect task/spec changes after draft and block Start until
  // refreshed or explicitly accepted"): when Start refuses with `stale`, this
  // card shows a warning and an explicit "Start anyway" action instead of the
  // ordinary Start button. Cleared on refresh (a fresh mount re-fetches the
  // lead, which no longer carries this outcome) and on a fresh Start attempt.
  const [stale, setStale] = useState<{ currentFingerprint: string } | null>(null)
  // Source-kickoffs 2.4/5.3: a Start that failed for a fixable reason stays
  // on this card (with Try again and the matching fix) instead of flashing
  // past as a toast. Cleared when a Start succeeds or the person closes it.
  const [failure, setFailure] = useState<StartFailureCardModel | null>(null)
  const openRepo = useOpenRepo()
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
        setFailure(null)
        refreshSession()
        toast.success(
          outcome.started_helpers.length > 0
            ? `Started the lead and ${outcome.started_helpers.length} helper${outcome.started_helpers.length === 1 ? '' : 's'}.`
            : 'Started the lead agent.'
        )
        return
      }
      if (outcome.kind === 'noHelpersRunSolo') {
        // A plan with no helpers in it is the lead saying it will do the
        // work itself. The plan is cleared and the chat is ready for its
        // next message; nothing was started, and nothing went wrong.
        setStale(null)
        setFailure(null)
        refreshSession()
        toast.info('This plan has no helpers, so the agent will work on its own.', {
          description: 'Send your next message when you are ready.',
        })
        return
      }
      if (outcome.kind === 'stale') {
        // Refuses outright: no worktree was provisioned, nothing was
        // written. The user must refresh (re-plan) or explicitly accept the
        // drift before Start will proceed -- see the warning banner below.
        setFailure(null)
        setStale({ currentFingerprint: outcome.current_fingerprint })
        toast.warning('The OpenSpec source changed since this plan was drafted.', {
          description: 'Review what changed, then start anyway or ask for a fresh plan.',
        })
        return
      }
      const card = startFailureCardForGraph(outcome)
      if (card) {
        setFailure(card)
        return
      }
      if (outcome.kind === 'alreadyStarted') {
        // A racing click already won. Refresh so the view shows what it
        // actually started; this card unmounts once the proposal is gone.
        refreshSession()
        toast.info('This plan was already started.')
      } else if (outcome.kind === 'noProposal') {
        toast.error('There is no plan waiting to start.')
      } else if (outcome.kind === 'notFound') {
        toast.error('This chat is gone. It may have been archived elsewhere.')
      } else if (outcome.kind === 'damaged') {
        toast.error('This chat file is damaged and could not start.', { description: outcome.reason })
      }
    } catch (e) {
      const message = describeError(e)
      log.error(`agent desk: could not start graph: ${message}`)
      setFailure(startFailureCardForError(message))
    } finally {
      setBusy(null)
    }
  }

  /**
   * "Open project" on a sourceMissing card: open the chat's repository as a
   * tab, then run the same `start()` every other Start uses. See
   * `SessionComposer.openProjectAndStart` for why the start follows at once.
   */
  const openProjectAndStart = async () => {
    if (busy) return
    setBusy('opening')
    try {
      await openRepo.mutateAsync(session.header.repoPath)
    } catch {
      // `useOpenRepo` already toasted the reason; the card stays.
      setBusy(null)
      return
    }
    setBusy(null)
    await start()
  }

  const onFailureAction = (action: StartFailureAction) => {
    if (action === 'tryAgain') void start()
    else if (action === 'openProject') void openProjectAndStart()
    // 'pickProvider' is never offered for a graph start: the plan already
    // fixed its tool when it was drafted, and the mapping does not emit it.
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
        toast.error('Could not accept the source change.', { description: describeOutcomeKind(outcome.kind) })
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
        toast.error('Could not switch to a single agent.', { description: describeOutcomeKind(outcome.kind) })
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
      <div className="flex items-center gap-1.5 text-2xs font-bold uppercase tracking-wide text-muted-foreground">
        <ListTodo size={11} aria-hidden />
        Plan ready to review
      </div>
      <p className="mt-1.5 text-2xs leading-relaxed text-foreground">{proposal.leadSummary}</p>
      {/*
        Each helper's job AND which of your files it may change. This list
        used to show the title and the stored role token only, so someone
        pressing Start authorised file-writing agents without being shown the
        boundary the backend then enforces against them.
      */}
      <ul className="mt-1.5 flex flex-col gap-1">
        {proposal.helpers.map((h) => {
          const scope = allowedPathLines(h.allowedPaths)
          return (
          <li key={h.nodeId} className="rounded border border-border bg-panel2 px-1.5 py-1 text-2xs">
            <span className="font-semibold text-foreground">{h.title}</span>
            <span className="text-muted-foreground"> · {helperRoleLabel(h.role)}</span>
            {/*
              A line per path. Joined into one truncated line, three realistic
              paths run to ~155 characters and this panel can be 360px wide, so
              the reader saw the first path and an ellipsis -- on the control
              that grants those exact paths. Nothing is cut mid-path now; what
              does not fit is counted.
            */}
            <span className="mt-0.5 block text-2xs text-sub">
              {h.allowedPaths.length === 0 ? (
                <>Can change: {allowedPathsLabel(h.allowedPaths)}</>
              ) : (
                <>
                  Can change:
                  {scope.lines.map((path) => (
                    <span key={path} className="block truncate pl-2 font-mono" title={path}>
                      {path}
                    </span>
                  ))}
                  {scope.rest > 0 && <span className="block pl-2">and {scope.rest} more</span>}
                </>
              )}
            </span>
          </li>
          )
        })}
      </ul>
      {stale && (
        // R5.4/tasks.md 3.3: Start already refused once for this exact
        // drift -- this banner is why, and it stays visible (rather than a
        // one-shot toast) until the user either revises or explicitly
        // starts anyway, since a toast alone would be gone before someone
        // reads it if they stepped away from the window.
        <div className="mt-2 flex items-start gap-1.5 rounded border border-[var(--gw-amber)]/40 bg-[var(--gw-amber)]/10 px-2 py-1.5 text-2xs leading-relaxed text-[var(--gw-amber)]">
          <TriangleAlert size={12} className="mt-px flex-none" aria-hidden />
          <span>
            The spec this plan came from was edited after the plan was written, so the plan may not match it any
            more. GitWyrm can tell that it changed but not what changed -- open the spec in Spec Desk to compare.
            Then either ask for a fresh plan, or start anyway with the plan as written.
          </span>
        </div>
      )}
      {failure && (
        <div className="mt-2">
          <StartFailureCard
            card={failure}
            busy={busy !== null}
            onAction={onFailureAction}
            onDismiss={() => setFailure(null)}
          />
        </div>
      )}
      <div className="mt-2 flex gap-1.5">
        {stale ? (
          <button
            type="button"
            onClick={() => void startAnyway()}
            disabled={busy !== null}
            className="rounded bg-primary px-2 py-1 text-2xs font-semibold text-primary-foreground disabled:cursor-not-allowed disabled:opacity-50"
          >
            {busy === 'accepting' || busy === 'start' ? 'Starting…' : 'Start anyway'}
          </button>
        ) : (
          <button
            type="button"
            onClick={() => void start()}
            disabled={busy !== null}
            className="rounded bg-primary px-2 py-1 text-2xs font-semibold text-primary-foreground disabled:cursor-not-allowed disabled:opacity-50"
          >
            {busy === 'start' ? 'Starting…' : 'Start'}
          </button>
        )}
        <button
          type="button"
          onClick={onRevise}
          disabled={busy !== null}
          className="rounded border border-border bg-panel2 px-2 py-1 text-2xs font-semibold text-foreground hover:bg-panel3 disabled:cursor-not-allowed disabled:opacity-50"
        >
          {stale ? 'Revise plan' : 'Revise'}
        </button>
        <button
          type="button"
          onClick={() => void useSolo()}
          disabled={busy !== null}
          className="rounded border border-border bg-panel2 px-2 py-1 text-2xs font-semibold text-foreground hover:bg-panel3 disabled:cursor-not-allowed disabled:opacity-50"
        >
          {busy === 'solo' ? 'Switching…' : 'Use solo instead'}
        </button>
      </div>
    </div>
  )
}
