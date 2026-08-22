import { useEffect, useRef } from 'react'
import { toast } from 'sonner'
import { commands } from '@/lib/bindings'
import { unwrap } from '@/lib/queryKeys'
import { describeError, log } from '@/lib/log'

/**
 * P1-C wiring 3 ("orphan-result detection is registered but not called at
 * startup"): `commands::agent_result::agent_result_find_orphaned` (and its
 * whole-store sibling `agent_result_find_orphaned_all`, added alongside this
 * hook) existed and worked correctly, but nothing in the app ever called
 * either one. A run that crashed, or whose worktree was deleted by hand,
 * while its result was `Kept`/`CleanupNeeded` would sit that way silently
 * forever -- opening the app again gave no signal that a result's worktree
 * folder was gone, so a user reviewing that session later would only
 * discover it when Undo/Commit/cleanup itself failed with no context.
 *
 * This hook calls the whole-store scan once, the first time Agent Desk's
 * window mounts (the `useRef` guard is `StrictMode`-safe: the effect body
 * still runs twice in dev, but the second run sees `ranRef.current === true`
 * and skips the call rather than double-toasting), and surfaces a single
 * plain-language toast naming how many orphaned results were found across
 * however many sessions -- not a toast per orphan, which would be noisy for
 * a bulk case like an OS reinstall wiping every worktree at once.
 *
 * Deliberately does not attempt any repair itself: task 5.3's own stance
 * ("flagged rather than silently dropped, without deleting automatically")
 * means the fix is a person's call, not this hook's -- it only makes the
 * problem visible. The toast's action opens the affected session's
 * conversation, where `ResultReviewPanel` already renders the ordinary
 * Undo/cleanup affordances for a result in this shape; no new UI surface is
 * needed to act on what this hook finds.
 */
export function useOrphanResultReconciliation(onOpenSession?: (sessionId: string) => void): void {
  const ranRef = useRef(false)

  useEffect(() => {
    if (ranRef.current) return
    ranRef.current = true

    void (async () => {
      try {
        const orphans = unwrap(await commands.agentResultFindOrphanedAll())
        if (orphans.length === 0) return

        const sessionCount = new Set(orphans.map((o) => o.sessionId)).size
        const description =
          sessionCount === 1
            ? "Its saved work is still recorded, but the folder it made changes in is gone -- you'll need to redo or discard it."
            : "Their saved work is still recorded, but the folders they made changes in are gone -- you'll need to redo or discard each one."

        toast.warning(
          orphans.length === 1
            ? 'One chat has changes that were kept, but their workspace folder is missing.'
            : `${orphans.length} kept results are missing their workspace folder, across ${sessionCount} chats.`,
          {
            description,
            action: onOpenSession
              ? {
                  label: 'Open the first one',
                  onClick: () => onOpenSession(orphans[0].sessionId),
                }
              : undefined,
          }
        )
      } catch (e) {
        // Best-effort reconciliation: a failure here means the user simply
        // does not get the heads-up this run, not that anything is broken --
        // the orphan (if any) is still on disk exactly as it was, and this
        // same scan runs again next launch.
        log.error(`agent desk: could not scan for orphaned results at startup: ${describeError(e)}`)
      }
    })()
  }, [onOpenSession])
}
