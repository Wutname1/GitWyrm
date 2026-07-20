import { toast } from 'sonner'
import { log } from '@/lib/log'

/**
 * Copy text and tell the user what happened.
 *
 * Every copy in the app goes through here so a failure can never be reported
 * as a success: the toast fires after the write resolves, not alongside it.
 * The clipboard API rejects when the document isn't focused or permission is
 * refused, and it is absent entirely in some webview contexts.
 */
export async function copyToClipboard(text: string, successMessage: string): Promise<boolean> {
  try {
    if (!navigator.clipboard) throw new Error('clipboard unavailable')
    await navigator.clipboard.writeText(text)
    toast(successMessage)
    return true
  } catch (e) {
    log.warn(`clipboard write failed: ${(e as Error).message}`)
    toast.error("Could not copy - your system didn't allow it")
    return false
  }
}
