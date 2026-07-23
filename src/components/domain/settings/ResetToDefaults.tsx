import { RotateCcw } from 'lucide-react'
import { toast } from 'sonner'
import { Button } from '@/components/ui/button'
import {
  useWorkspaceStore,
  type SettingsGroup,
  type SettingsSnapshot,
} from '@/stores/workspaceStore'

/**
 * Per-screen "Reset to defaults" button. Resets the screen's settings right
 * away and raises an undo toast holding the prior values, so the reset is
 * reversible without a confirm dialog.
 *
 * Pass `group` for an app-wide settings screen, or `onReset` for a screen with
 * its own reset logic (e.g. per-repo) that returns an undo callback.
 */
export function ResetToDefaults({
  group,
  onReset,
  label = 'Reset this page to defaults',
  message = 'This page was reset to defaults',
  disabled = false,
}: {
  group?: SettingsGroup
  onReset?: () => (() => void) | null
  label?: string
  message?: string
  disabled?: boolean
}) {
  const resetSettingsGroup = useWorkspaceStore((s) => s.resetSettingsGroup)
  const restoreSettings = useWorkspaceStore((s) => s.restoreSettings)

  const handle = () => {
    if (onReset) {
      const undo = onReset()
      toast.success(message, undo ? { action: { label: 'Undo', onClick: undo }, duration: 6000 } : undefined)
      return
    }
    if (group) {
      const snapshot = resetSettingsGroup(group)
      showUndoToast(snapshot, restoreSettings, message)
    }
  }

  return (
    <div className="mt-6 border-t border-border pt-4">
      <Button variant="ghost" size="sm" onClick={handle} disabled={disabled}>
        <RotateCcw size={13} />
        {label}
      </Button>
    </div>
  )
}

/** Raise the shared undo toast for a reset. Exported for the global reset too. */
export function showUndoToast(
  snapshot: SettingsSnapshot,
  restore: (snapshot: SettingsSnapshot) => void,
  message: string,
) {
  toast.success(message, {
    action: {
      label: 'Undo',
      onClick: () => restore(snapshot),
    },
    duration: 6000,
  })
}
