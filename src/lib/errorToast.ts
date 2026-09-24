import { toast } from 'sonner'
import type { ClassifiedError } from '@/lib/errorClass'
import { useUiStore } from '@/stores/uiStore'

/**
 * Shows a classified error at its severity. When the fix lives on a settings
 * page the toast carries a button that goes straight there.
 */
export function showErrorToast({ severity, message, fix }: ClassifiedError) {
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
