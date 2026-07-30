import { toast } from 'sonner'
import { commands } from '@/lib/bindings'
import { unwrap } from '@/lib/queryKeys'
import { describeError, log } from '@/lib/log'
import { selectChangeEverywhere } from '@/lib/specSync'

const inTauri = '__TAURI_INTERNALS__' in window

/**
 * Opens the Spec Desk window for a repository, or focuses the one already open.
 *
 * Every entry point (sidebar footer, spec card, and later the graph chips) funnels
 * through here, so there is one place that decides what "open the Desk" means.
 *
 * Selecting the change first means the Desk paints on the right change instead of
 * jumping a beat later, and the main window's card follows too.
 */
export async function openSpecDesk(repoId: string, changeId?: string) {
  if (changeId) {
    selectChangeEverywhere(changeId)
  }
  if (!inTauri) {
    toast.info('The Spec Desk opens as a separate window in the desktop app.')
    return
  }
  try {
    // The id goes over the wire as well as over the event: a Desk that is not
    // open yet has no listener, so the broadcast above only reaches an already
    // open one. The URL covers the first open.
    const outcome = unwrap(await commands.openSpecDesk(repoId, changeId ?? null))
    // Opening is self-evident -- a window appears. Focusing an already-open Desk
    // is not, especially when it is on another monitor, so say so.
    if (outcome === 'focused') {
      toast.info('The Spec Desk is already open - brought it to the front.')
    }
  } catch (e) {
    const message = describeError(e)
    log.error(`open spec desk failed: ${message}`)
    toast.error(`Could not open the Spec Desk. ${message}`)
  }
}
