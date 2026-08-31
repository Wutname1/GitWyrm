import { useEffect } from 'react'
import { listen } from '@tauri-apps/api/event'
import { toast } from 'sonner'

import type { GitProgressPayload } from '@/lib/bindings'

/**
 * Shows live progress for local git operations that rewrite the working tree.
 *
 * Merge, discard and checkout print nothing while they run, but they are the
 * slowest things the app does: a merge touching 302 files takes ~780ms, and
 * 10,000 files ~26s. Without this the window simply sits there, which reads as
 * a crash rather than as work in progress.
 *
 * One toast per operation, updated in place by id, so a long merge is a single
 * moving line rather than a stack of hundreds. The backend throttles emits to
 * ~10/s, so this does not re-render more often than that.
 */

/** Operations that report countable progress, mapped to their verb. */
const LABELS: Record<string, string> = {
  merge: 'Merging',
  discard: 'Discarding changes',
  checkout: 'Switching branch',
}

export function useLocalGitProgress() {
  useEffect(() => {
    // Which operations currently own a toast, so the last update can dismiss
    // the right one. Keyed by repo+operation: two repos can merge at once.
    const open = new Set<string>()

    const unlisten = listen<GitProgressPayload>('git-progress', (event) => {
      const { repo_id, operation, line, completed, total } = event.payload
      const label = LABELS[operation]
      // Network operations (fetch/push/clone) stream free-text stderr and have
      // their own UI; only the local ones are handled here.
      if (!label) return

      const id = `git-progress:${repo_id}:${operation}`
      open.add(id)

      const done = total != null && completed != null && total > 0 && completed >= total
      if (done) {
        // The mutation's own success toast reports the outcome, so this one
        // just gets out of the way rather than adding a second message.
        toast.dismiss(id)
        open.delete(id)
        return
      }

      toast.loading(total != null && total > 0 ? `${label} - ${line}` : `${label}...`, { id })
    })

    return () => {
      void unlisten.then((stop) => stop())
      // A reload mid-merge would otherwise strand a spinner that never clears.
      for (const id of open) toast.dismiss(id)
    }
  }, [])
}
