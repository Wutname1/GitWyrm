import { useEffect, useState } from 'react'
import { toast } from 'sonner'
import { commands } from '@/lib/bindings'
import { unwrap } from '@/lib/queryKeys'
import { isWindows } from '@/lib/platform'
import { SettingRow } from './SettingRow'

/**
 * Toggle for Explorer's "Open with GitWyrm" right-click entry.
 *
 * The registry is the source of truth, not settings.json: the installer writes
 * these keys and the uninstaller removes them, so a value cached in our own
 * settings would drift the first time someone reinstalls. This reads the live
 * state on mount and writes straight through.
 *
 * Windows only. The backend command errors off Windows, and the read returns
 * false there, so without this gate a Linux user would see a switch that
 * reports itself off and then fails with an error toast when clicked. Linux
 * file managers each want their own action file rather than a registry key.
 */
export function ContextMenuSetting() {
  const [enabled, setEnabled] = useState<boolean | null>(null)
  const [busy, setBusy] = useState(false)

  useEffect(() => {
    // Nothing to read off Windows, and the row does not render there anyway.
    if (!isWindows()) return
    let active = true
    void (async () => {
      try {
        const registered = unwrap(await commands.contextMenuRegistered())
        if (active) setEnabled(registered)
      } catch {
        // Reading the registry should not fail, but if it does the toggle just
        // shows as off rather than blocking the whole settings screen.
        if (active) setEnabled(false)
      }
    })()
    return () => {
      active = false
    }
  }, [])

  const toggle = async (next: boolean) => {
    setBusy(true)
    // Move the switch immediately so the click registers, then reconcile with
    // whatever the registry actually ends up saying.
    setEnabled(next)
    try {
      unwrap(await commands.setContextMenuRegistered(next))
      toast.success(
        next
          ? 'GitWyrm was added to your right-click menu'
          : 'GitWyrm was removed from your right-click menu',
      )
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error)
      toast.error(`Could not change the right-click menu: ${message}`)
    } finally {
      // Trust the registry over our optimistic guess.
      try {
        setEnabled(unwrap(await commands.contextMenuRegistered()))
      } catch {
        setEnabled(!next)
      }
      setBusy(false)
    }
  }

  // After the hooks, never before: an early return above them would change the
  // hook order between platforms.
  if (!isWindows()) return null

  return (
    <SettingRow
      label="Right-click menu"
      searchId="context-menu"
      hint="Add Open with GitWyrm when you right-click a folder in File Explorer."
    >
      <label className="flex cursor-pointer items-center gap-2 text-xs text-foreground">
        <input
          type="checkbox"
          checked={enabled ?? false}
          disabled={enabled === null || busy}
          onChange={(e) => void toggle(e.target.checked)}
          className="size-3.5 accent-[var(--gw-accent)]"
        />
        Add GitWyrm to my right-click menu
      </label>
    </SettingRow>
  )
}
