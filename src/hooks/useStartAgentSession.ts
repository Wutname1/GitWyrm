import { useState } from 'react'
import { useQueryClient } from '@tanstack/react-query'
import { toast } from 'sonner'
import {
  commands,
  type AgentSession,
  type SessionIntent,
  type SessionSourceInput,
  type StartAgentSessionOutcome,
} from '@/lib/bindings'
import { keys, unwrap } from '@/lib/queryKeys'
import { describeError, log } from '@/lib/log'
import { openSpecDesk } from '@/lib/specDesk'
import { explainAutoStartOutcome } from '@/lib/agentDeskResult'

/**
 * P1-A fix ("source clicks do not all do what they say"): starting a
 * session's first turn is no longer decided here at all. It used to be --
 * this function read `IntentPolicy.canWrite` and skipped starting for any
 * read-only intent (Ask/Explain/Review/Summarize), so "Review with AI"
 * created a session that then sat in `Draft` forever, looking like it
 * worked (a chat appeared) while nothing ever ran. That was the wrong test:
 * "can this intent ever write" is a CAPABILITY question the backend's
 * `agentdesk::policy::check_tool_capability` already enforces on every tool
 * call inside a run; it says nothing about whether the run itself should
 * start.
 *
 * The backend's `agent_session_start` command (`commands::agent_kickoff`)
 * now performs the create-and-start sequence in one call, for every intent,
 * carrying whatever `mode`/`team`/`providerOverride` the caller (or the
 * intent's own policy default) named -- so `StartAgentSessionOutcome::Created`
 * already carries `start: StartExecutionOutcome | null` by the time it gets
 * here. This function only reports that outcome; `explainAutoStartOutcome`
 * (`@/lib/agentDeskResult`) is the same plain-language mapping the old
 * frontend-driven start used, reused verbatim so the messages a user sees
 * have not changed even though which layer decided to start has.
 */
function reportCreatedStart(start: NonNullable<Extract<StartAgentSessionOutcome, { kind: 'created' }>['start']>): void {
  if (start.kind === 'started') {
    // Rule #1: the session already shows "Preparing" the instant it was
    // created (`record_execution_if_not_running` sets that state inside the
    // same locked write `start_execution_at` performs, now called directly
    // from `agent_session_start`) -- this toast confirms the engine itself
    // is now running, not just queued to.
    toast.success('Started working on this.')
    return
  }
  const explanation = explainAutoStartOutcome(start)
  if (explanation) toast.error('Could not start working on this.', { description: explanation })
  // `explanation === null` and `start.kind !== 'started'` only for
  // `alreadyRunning`: an execution is already active on this session (a
  // near-simultaneous second kickoff, most likely) -- it is already visibly
  // Working, which is the same state this call would have produced, so
  // nothing more to say.
}

export interface StartAgentSessionRequest {
  repoId: string
  repoPath: string
  repoName: string
  source: SessionSourceInput
  intent: SessionIntent
  /** A specific provider id for the "…with" secondary override (task
   * 3.2/4.2), bypassing the intent's policy default. Omitted for the
   * primary action. */
  providerOverride?: string
  /** Identifies which caller-side action is in flight, e.g. `pr:318:review`,
   * so a button can show its own busy state without a global spinner. */
  key: string
}

/**
 * The one shared kickoff path every source surface (issue row, PR row,
 * OpenSpec task, ...) uses to start or focus an Agent Desk session --
 * architecture.md section 8's `StartAgentSessionRequest`, and the exact
 * feedback order build-order.md's Gate 3 tests against a deliberately slow
 * host/provider:
 *
 *   a. the clicked source shows "Starting" immediately, before awaiting
 *      anything (`startingKey`, set synchronously below);
 *   b. Agent Desk opens/focuses;
 *   c. the session (new, or an existing active one for the same
 *      repo/source/intent -- task 1.4) is created durably, which is what
 *      puts a fresh session in `Draft`/`Preparing` before any provider
 *      call;
 *   d. Starting clears on every success/failure/unmount path (the `finally`
 *      below).
 *
 * P1-A: `commands.agentSessionStart` (backend: `agent_kickoff::agent_session_start`)
 * now performs BOTH create and first-turn-start as one call for a freshly
 * created session -- every explicit source action starts immediately, for
 * every intent, not only the ones that happen to be able to write. This
 * hook no longer decides whether to start a second time on the frontend
 * (there used to be a separate `agentSessionStartExecution` call here,
 * gated on `IntentPolicy.canWrite`, which is exactly what left Review/
 * Summarize sessions sitting in `Draft` forever) -- it only reports the
 * `start` outcome the backend already attempted, via `reportCreatedStart`.
 *
 * Routes through `commands.agentSessionStart` (backend:
 * `commands::agent_kickoff::agent_session_start`), not the lower-level
 * `agentSessionCreate` -- the kickoff command is what performs the
 * duplicate-session lookup (design.md "Deduplication": re-clicking Fix on
 * the same issue focuses the existing session rather than forking a second
 * one) and stamps the source snapshot itself, so a caller cannot construct
 * a `SessionSource` that lies about when it was captured.
 *
 * Does not fetch anything the caller does not already have: `source` is
 * built from already-loaded row/detail data by `agentDeskSources.ts`, per
 * "Do not fetch all source details in the main window before opening Agent
 * Desk."
 */
export function useStartAgentSession() {
  const qc = useQueryClient()
  const [startingKey, setStartingKey] = useState<string | null>(null)

  const startSession = async (request: StartAgentSessionRequest): Promise<AgentSession | null> => {
    if (startingKey) return null
    // Step (a): visible acknowledgement before awaiting anything.
    setStartingKey(request.key)
    // `startingKey` only reaches the eye when the component that read it is
    // still mounted. A context menu unmounts the moment an item is chosen, so
    // on that path -- the right-click gesture the product leads with -- it
    // renders into nothing and the app appears to ignore the click. A toast
    // outlives the menu, so it is the acknowledgement that always lands. Every
    // outcome below replaces it by id rather than stacking a second one.
    const ackId = toast.loading('Starting a chat for this…')
    try {
      // Step (b): open/focus Agent Desk concurrently with the durable
      // create below -- neither needs to wait on the other, and a slow
      // window focus must never delay the create Gate 3 requires to be
      // immediate.
      const openDesk = openSpecDesk(request.repoId)

      // Step (c): create or focus the durable session. Fast, local,
      // disk-only -- no provider or host round-trip.
      const outcome: StartAgentSessionOutcome = unwrap(
        await commands.agentSessionStart({
          repoId: request.repoId,
          repoPath: request.repoPath,
          repoName: request.repoName,
          source: request.source,
          intent: request.intent,
          mode: null,
          team: null,
          providerOverride: request.providerOverride ?? null,
        })
      )

      await openDesk
      void qc.invalidateQueries({ queryKey: keys.agentSessionsAll })

      switch (outcome.kind) {
        case 'created':
          // R3.2: no second user message required. The backend has already
          // attempted the start by the time this resolves (P1-A) --
          // `outcome.start` is that attempt's own outcome (`null` only in a
          // forward-compatibility case that does not occur in this build,
          // see `StartAgentSessionOutcome::Created`'s doc comment).
          toast.dismiss(ackId)
          if (outcome.start) reportCreatedStart(outcome.start)
          return outcome.session
        case 'focusedExisting':
          toast.info('Already working on this. Focused the existing chat.', { id: ackId })
          return outcome.session
        case 'writeFailed':
          toast.error('Could not start a chat for this.', { id: ackId, description: outcome.detail })
          return null
        default:
          toast.dismiss(ackId)
          return null
      }
    } catch (e) {
      const message = describeError(e)
      log.error(`agent desk: could not start session from source: ${message}`)
      toast.error('Could not start a chat for this.', { id: ackId, description: message })
      return null
    } finally {
      // Step (d): clear on every path, success or failure.
      setStartingKey(null)
    }
  }

  return { startSession, starting: startingKey !== null, startingKey }
}
