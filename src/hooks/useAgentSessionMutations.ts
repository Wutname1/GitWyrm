import { useMutation, useQueryClient } from '@tanstack/react-query'
import { toast } from 'sonner'
import { commands } from '@/lib/bindings'
import { keys, unwrap } from '@/lib/queryKeys'
import { log } from '@/lib/log'

/**
 * Sidebar-scoped session mutations (rename, archive) for tasks 3.5's actions.
 *
 * Separate from a hypothetical `useAgentSessions` mutation set because the
 * sidebar is the only cluster-C surface that performs these -- keeping them
 * here avoids reaching into `useAgentSessions.ts`, which this cluster does
 * not own (see the shell-scaffold brief's do-not-touch list).
 *
 * Every mutation invalidates the session-list query (so the row updates) and
 * the single-session query (so an open conversation pane reflects a rename),
 * matching the invalidation shape `useAgentSessionListener` already uses for
 * live events.
 */
export function useAgentSessionMutations() {
  const qc = useQueryClient()

  const invalidate = (sessionId: string) => {
    qc.invalidateQueries({ queryKey: keys.agentSessionsAll })
    qc.invalidateQueries({ queryKey: keys.agentSession(sessionId) })
  }

  const rename = useMutation({
    mutationFn: async (vars: { sessionId: string; title: string }) =>
      unwrap(await commands.agentSessionRename(vars.sessionId, vars.title)),
    onSuccess: (outcome, vars) => {
      invalidate(vars.sessionId)
      if (outcome.kind !== 'updated') {
        log.warn(`agent session rename: ${outcome.kind} for ${vars.sessionId}`)
        toast.error('Could not rename that chat.', {
          description: renameFailureReason(outcome.kind),
        })
      }
    },
    onError: (e, vars) => {
      log.error(`agent session rename threw: ${String(e)}`)
      toast.error('Could not rename that chat.')
      invalidate(vars.sessionId)
    },
  })

  const archive = useMutation({
    mutationFn: async (vars: { sessionId: string; archived: boolean }) =>
      unwrap(await commands.agentSessionArchive(vars.sessionId, vars.archived)),
    onSuccess: (outcome, vars) => {
      invalidate(vars.sessionId)
      if (outcome.kind !== 'updated') {
        log.warn(`agent session archive: ${outcome.kind} for ${vars.sessionId}`)
        toast.error(vars.archived ? 'Could not archive that chat.' : 'Could not restore that chat.')
      }
    },
    onError: (e, vars) => {
      log.error(`agent session archive threw: ${String(e)}`)
      toast.error(vars.archived ? 'Could not archive that chat.' : 'Could not restore that chat.')
      invalidate(vars.sessionId)
    },
  })

  const markRead = useMutation({
    mutationFn: async (sessionId: string) => unwrap(await commands.agentSessionMarkRead(sessionId)),
    onSuccess: (_outcome, sessionId) => invalidate(sessionId),
    onError: (e) => log.error(`agent session mark-read threw: ${String(e)}`),
  })

  return { rename, archive, markRead }
}

function renameFailureReason(kind: string): string {
  switch (kind) {
    case 'notFound':
      return 'That chat no longer exists.'
    case 'damaged':
      return 'Its saved file could not be read.'
    case 'unavailable':
      return 'Its saved file is locked right now. Try again in a moment.'
    default:
      return 'Its saved file could not be written.'
  }
}
