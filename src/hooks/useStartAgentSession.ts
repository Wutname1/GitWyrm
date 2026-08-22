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
 * R3.2: "After persistence and worktree provisioning, start execution
 * automatically; no second user message is required."
 *
 * Fires the same `agentSessionStartExecution` call `SessionComposer`'s Send
 * button makes, using the intent's own policy default mode/team
 * (`commands::agentdesk::policy::for_intent`) rather than inventing a second
 * default here. Only for a *freshly created* session -- `FocusedExisting`
 * (R3.4's duplicate-source case) must never restart an execution the user
 * may already be reviewing or mid-conversation with; the caller focuses that
 * session and stops, exactly as before.
 *
 * Deliberately does not gate on `intent === 'fix'` by name: the correct test
 * is "can this intent write at all" (`IntentPolicy.canWrite`), which is also
 * true for Plan once a user has explicitly chosen Start (this hook is not
 * that path) -- but false for Ask/Explain/Review/Summarize, which must show
 * the user their new session and let *them* decide whether/how to run it
 * (R1.3/R1.4: read-only intents are never executed without an explicit
 * user action). Reading the real policy table over the wire, instead of
 * hand-copying "fix" as a magic string, is what keeps this in sync with R1's
 * enforcement rather than silently drifting from it.
 */
async function autoStartIfWriteCapable(session: AgentSession): Promise<void> {
  let policy
  try {
    policy = await commands.agentIntentPolicy(session.header.intent)
  } catch (e) {
    // The policy lookup itself is read-only and side-effect-free; a failure
    // here just means we cannot safely decide whether to auto-start, so we
    // don't. The session is still visible and the user can send a message
    // manually -- this is a degraded-but-safe fallback, not a hard failure.
    log.error(`agent desk: could not resolve intent policy for auto-start: ${describeError(e)}`)
    return
  }
  if (!policy.canWrite) return

  try {
    const outcome = unwrap(
      await commands.agentSessionStartExecution(
        session.header.sessionId,
        policy.defaultMode,
        policy.defaultTeam,
        null
      )
    )
    if (outcome.kind === 'started') {
      // Rule #1: the session already shows "Preparing" the instant it was
      // created (`record_execution_if_not_running` sets that state inside
      // the same locked write `agentSessionStartExecution` performs) --
      // this toast confirms the engine itself is now running, not just
      // queued to.
      toast.success('Started working on this.')
      return
    }
    const explanation = explainAutoStartOutcome(outcome)
    if (explanation) toast.error('Could not start working on this.', { description: explanation })
    // `explanation === null` and `outcome.kind !== 'started'` only for
    // `alreadyRunning`: an execution is already active on this session (a
    // near-simultaneous second kickoff, most likely) -- it is already
    // visibly Working, which is the same state this call would have
    // produced, so nothing more to say.
  } catch (e) {
    const message = describeError(e)
    log.error(`agent desk: auto-start execution failed: ${message}`)
    toast.error('Could not start working on this.', { description: message })
  }
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
          // R3.2: no second user message required -- fire-and-forget so a
          // slow provider/worktree provisioning never blocks this function
          // from returning the session (which the caller uses to focus the
          // Desk pane immediately, per R3.1). Errors are reported by
          // `autoStartIfWriteCapable` itself via toast; nothing here awaits
          // it or can regress the "session shows up in Preparing right away"
          // guarantee into "session shows up only once the agent finishes."
          void autoStartIfWriteCapable(outcome.session)
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
