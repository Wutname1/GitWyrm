import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { commands } from '@/lib/bindings'
import { invalidateOpenspec, keys, unwrap } from '@/lib/queryKeys'
import { log } from '@/lib/log'

/**
 * The context a lead agent reads to understand an `openSpecChange`/
 * `openSpecTask` session's source: proposal, design, deltas, tasks,
 * progress, history, and (for a task source) the exact target task located
 * by identity rather than position (`agent-desk-openspec-workflows`
 * tasks.md section 2).
 *
 * Disabled for any other source kind -- callers gate rendering on
 * `session.header.source.kind` before mounting whatever uses this, same
 * pattern as `useOpenspecHistory`'s `enabled` guard.
 */
export function useOpenSpecSessionContext(sessionId: string | null, enabled = true) {
  return useQuery({
    queryKey: keys.agentSessionOpenspecContext(sessionId ?? 'none'),
    enabled: sessionId != null && enabled,
    queryFn: async () => unwrap(await commands.agentSessionOpenspecContext(sessionId!)),
  })
}

/**
 * Active/archived/moved/deleted status for an `openSpecChange`/
 * `openSpecTask` session source (tasks.md 4.5, section 7). Each state is
 * honest and carries what the UI needs for a real next action -- an
 * archived change is not an error, a moved one names its likely new id.
 */
export function useOpenSpecSessionStatus(sessionId: string | null, enabled = true) {
  return useQuery({
    queryKey: keys.agentSessionOpenspecStatus(sessionId ?? 'none'),
    enabled: sessionId != null && enabled,
    queryFn: async () => unwrap(await commands.agentSessionOpenspecStatus(sessionId!)),
  })
}

/**
 * Routes an accepted execution's task completion through the same checkbox
 * writer Spec Desk's own toggle uses (`crate::openspec::write::toggle_task_line`),
 * then refreshes every surface that shows this session or this repo's
 * OpenSpec progress -- tasks.md 4.3/R5.6: "Refresh main window, Desk source,
 * progress, transcript, and graph projections."
 *
 * The `agentSession` invalidation covers "transcript" (the session document
 * carries `messages`) and "graph" (a lead's `ExecutionRecord.proposedGraph`/
 * `conflict` live on the same session document -- there is no separate graph
 * query to invalidate). `invalidateOpenspec` is the same helper the main
 * window's own OpenSpec writes use, so a task ticked from an accepted Agent
 * Desk run moves the main window's change list/progress counts exactly like
 * a click on the Spec Desk checkbox would.
 *
 * Never call this from an execution-finished handler alone -- tasks.md 4.4
 * requires the caller to have already gotten explicit user acceptance first;
 * this mutation only performs the write once that acceptance exists.
 */
export function useCompleteOpenSpecTask(repoId: string | null) {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: async (vars: { sessionId: string; done: boolean }) =>
      unwrap(await commands.agentSessionCompleteOpenspecTask(vars.sessionId, vars.done)),
    onSuccess: (outcome, vars) => {
      qc.invalidateQueries({ queryKey: keys.agentSession(vars.sessionId) })
      qc.invalidateQueries({ queryKey: keys.agentSessionsAll })
      qc.invalidateQueries({ queryKey: keys.agentSessionOpenspecContext(vars.sessionId) })
      qc.invalidateQueries({ queryKey: keys.agentSessionOpenspecStatus(vars.sessionId) })
      if (repoId) {
        invalidateOpenspec(qc, repoId)
      }
      if (outcome.kind !== 'completed') {
        log.warn(`complete openspec task: ${outcome.kind} for ${vars.sessionId}`)
      }
    },
    onError: (e, vars) => {
      log.error(`complete openspec task threw: ${String(e)} for ${vars.sessionId}`)
    },
  })
}
