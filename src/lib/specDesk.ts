import { toast } from 'sonner'
import { commands } from '@/lib/bindings'
import { unwrap } from '@/lib/queryKeys'
import { describeError, log } from '@/lib/log'
import { selectChangeEverywhere } from '@/lib/specSync'

const inTauri = '__TAURI_INTERNALS__' in window

/**
 * Opens the app-wide Agent Desk window, or focuses it if it is already open.
 *
 * Every entry point (sidebar footer, spec card, and later the graph chips) funnels
 * through here, so there is one place that decides what "open the Desk" means.
 *
 * Kept as `openSpecDesk` -- not renamed to `openAgentDesk` -- because six call
 * sites already import it by this name (Toolbar, OpenSpecDeskButton, SpecCard,
 * LeftPanel, SpecChip, SpecContextMenu) and the persisted setting it reads is
 * `enableSpecDesk`/`enable_spec_desk`. Only the user-facing strings below say
 * "Agent Desk"; see `docs/agent-desk/architecture.md` section 6.
 *
 * Selecting the change first means the Desk paints on the right change instead of
 * jumping a beat later, and the main window's card follows too.
 */
export async function openSpecDesk(repoId: string, changeId?: string) {
  if (changeId) {
    selectChangeEverywhere(changeId)
  }
  if (!inTauri) {
    toast.info('Agent Desk opens as a separate window in the desktop app.')
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
      toast.info('Agent Desk is already open - brought it to the front.')
    }
  } catch (e) {
    const message = describeError(e)
    log.error(`open agent desk failed: ${message}`)
    toast.error(`Could not open Agent Desk. ${message}`)
  }
}
