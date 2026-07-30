import { toast } from 'sonner'
import { useUiStore } from '@/stores/uiStore'

/**
 * Opens (or focuses) the Spec Desk window for a repository.
 *
 * The window itself ships with `add-spec-desk-window`. Until then this is the
 * one place every entry point funnels through, so wiring the real window later
 * is a single edit rather than a hunt through the sidebar, the spec card, and
 * the graph chips.
 *
 * It still selects the change and says what will happen: a button that answers a
 * click with nothing reads as broken, and "coming soon" is more honest than a
 * dead control.
 */
export function openSpecDesk(_repoId: string, changeId?: string) {
  if (changeId) {
    useUiStore.getState().selectChange(changeId)
  }
  toast.info('The Spec Desk window is coming next. For now, spec status lives here.')
}
