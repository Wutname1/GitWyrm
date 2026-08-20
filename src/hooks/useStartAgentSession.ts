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
 *      call. `agent_session_start` never calls the provider itself, only
 *      `agent_session_start_execution` (a caller's later, separate step)
 *      does that;
 *   d. Starting clears on every success/failure/unmount path (the `finally`
 *      below).
 *
 * Deliberately NOT itself calling `agent_session_start_execution`: kickoff's
 * job is "get a session showing in Agent Desk right now," not "run the
 * agent" -- Ask/Explain/Review/Summarize/Plan may want the user to see the
 * session before anything executes.
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
          return outcome.session
        case 'focusedExisting':
          toast.info('Already working on this. Focused the existing chat.')
          return outcome.session
        case 'writeFailed':
          toast.error('Could not start a chat for this.', { description: outcome.detail })
          return null
        default:
          return null
      }
    } catch (e) {
      const message = describeError(e)
      log.error(`agent desk: could not start session from source: ${message}`)
      toast.error('Could not start a chat for this.', { description: message })
      return null
    } finally {
      // Step (d): clear on every path, success or failure.
      setStartingKey(null)
    }
  }

  return { startSession, starting: startingKey !== null, startingKey }
}
