import { toast } from 'sonner'
import type { ClassifiedError } from '@/lib/errorClass'
import { useUiStore } from '@/stores/uiStore'

/** Follow-up actions a caller can offer on its error toast. */
export interface ErrorActions {
  /** Run the same action again. Offered when the error says that can work. */
  retry?: () => void
  /** Put stashed changes back. Offered when the error says they were set aside. */
  restoreStash?: () => void
  /** Send the branch, which publishes and links it. Offered when it has no cloud branch. */
  push?: () => void
}

/**
 * Shows a classified error at its severity. When the fix lives on a settings
 * page the toast carries a button that goes straight there; when trying again
 * or putting changes back is the fix, and the caller can do it, the button does
 * that instead.
 */
export function showErrorToast(
  { severity, message, fix, retryable, stashKept, offerPush }: ClassifiedError,
  actions?: ErrorActions,
) {
  const followUp =
    stashKept && actions?.restoreStash
      ? { label: 'Put my changes back', onClick: actions.restoreStash }
      : offerPush && actions?.push
        ? { label: 'Send it', onClick: actions.push }
        : retryable && actions?.retry
          ? { label: 'Try again', onClick: actions.retry }
          : null
  if (followUp) {
    const options = { action: followUp, duration: 15000 }
    if (severity === 'info') toast.info(message, options)
    else if (severity === 'warning') toast.warning(message, options)
    else toast.error(message, options)
    return
  }
  const options = fix
    ? {
        action: {
          label: fix.label,
          onClick: () => {
            const ui = useUiStore.getState()
            if (fix.settingId) ui.revealSettingById(fix.section, fix.settingId)
            else ui.showSettings(fix.section)
          },
        },
        // Long enough to read the sentence and reach the button.
        duration: 10000,
      }
    : undefined
  if (severity === 'info') toast.info(message, options)
  else if (severity === 'warning') toast.warning(message, options)
  else toast.error(message, options)
}
