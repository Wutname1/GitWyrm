import { useMutation, useQueryClient } from '@tanstack/react-query'
import { toast } from 'sonner'
import { commands } from '@/lib/bindings'
import { keys, unwrap } from '@/lib/queryKeys'
import { log } from '@/lib/log'
import { explainEscalateToFixOutcome } from '@/lib/agentDeskResult'

/**
 * Sidebar-scoped session mutations (rename, archive, delete) for task 3.5's
 * actions.
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
        return
      }
      // Success is reported HERE, once it has actually happened. The row used
      // to toast "Archived" synchronously on click, before the mutation ran,
      // so a failure produced a green confirmation and a red refusal for the
      // same click and the person could not tell which was true.
      if (vars.archived) {
        toast.success('Chat archived.', {
          description: 'You can restore it from the Archived filter any time.',
        })
      } else {
        toast.success('Chat restored.')
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

  const remove = useMutation({
    mutationFn: async (sessionId: string) =>
      unwrap(await commands.agentSessionDelete(sessionId)),
    onSuccess: (outcome, sessionId) => {
      invalidate(sessionId)
      if (outcome.kind === 'stillRunning') {
        // Not an error: the chat is fine, it is just busy. Deleting the file
        // underneath a working agent would lose whatever it was part-way
        // through, and the person asking may not have known it was running.
        toast.error('That chat is still working.', {
          description: 'Stop it first, then delete it.',
        })
        return
      }
      if (outcome.kind === 'failed') {
        log.warn(`agent session delete failed for ${sessionId}: ${outcome.detail}`)
        toast.error('Could not delete that chat.', { description: outcome.detail })
      }
    },
    onError: (e, sessionId) => {
      log.error(`agent session delete threw for ${sessionId}: ${String(e)}`)
      toast.error('Could not delete that chat.')
    },
  })

  // Source-kickoffs task 4.6: "Fix this" on a finished read-only chat. The
  // backend creates a seeded Fix session and does NOT start it; the caller
  // (`ResultReviewPanel`) opens it in the pane so the person can read the
  // seeded message and press Send. Only the list is invalidated: the review
  // session itself is untouched, and the new session has never been queried.
  const escalateToFix = useMutation({
    mutationFn: async (sessionId: string) => unwrap(await commands.agentSessionEscalateToFix(sessionId)),
    onSuccess: (outcome, sessionId) => {
      qc.invalidateQueries({ queryKey: keys.agentSessionsAll })
      const explanation = explainEscalateToFixOutcome(outcome)
      if (explanation) {
        log.warn(`agent session escalate-to-fix: ${outcome.kind} for ${sessionId}`)
        toast.error('Could not start a fix chat.', { description: explanation })
      }
    },
    onError: (e, sessionId) => {
      log.error(`agent session escalate-to-fix threw for ${sessionId}: ${String(e)}`)
      toast.error('Could not start a fix chat.')
    },
  })

  return { rename, archive, markRead, remove, escalateToFix }
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
