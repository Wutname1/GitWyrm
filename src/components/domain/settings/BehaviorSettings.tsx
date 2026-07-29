import { useWorkspaceStore } from '@/stores/workspaceStore'
import { SettingRow } from './SettingRow'
import { ContextMenuSetting } from './ContextMenuSetting'
import { ResetToDefaults } from './ResetToDefaults'

export function BehaviorSettings() {
  const restoreTabs = useWorkspaceStore((s) => s.restoreTabs)
  const setRestoreTabs = useWorkspaceStore((s) => s.setRestoreTabs)
  const autoFetch = useWorkspaceStore((s) => s.autoFetch)
  const setAutoFetch = useWorkspaceStore((s) => s.setAutoFetch)

  return (
    <div>
      <SettingRow
        label="On startup"
        searchId="restore-tabs"
        hint="Reopen the repositories you had open when you last closed GitWyrm. Turn this off to start with a clean slate and pick a repository yourself."
      >
        <label className="flex cursor-pointer items-center gap-2 text-xs text-foreground">
          <input
            type="checkbox"
            checked={restoreTabs}
            onChange={(e) => setRestoreTabs(e.target.checked)}
            className="size-3.5 accent-[var(--gw-accent)]"
          />
          Reopen my last tabs
        </label>
      </SettingRow>
      <SettingRow
        label="Check for remote changes"
        searchId="auto-fetch"
        hint="Quietly check your remotes in the background so you can see when a branch is ahead or behind without pressing Fetch. This only downloads history -- it never changes your files."
      >
        <label className="flex cursor-pointer items-center gap-2 text-xs text-foreground">
          <input
            type="checkbox"
            checked={autoFetch}
            onChange={(e) => setAutoFetch(e.target.checked)}
            className="size-3.5 accent-[var(--gw-accent)]"
          />
          Check automatically
        </label>
      </SettingRow>
      {/* Registry-backed, so it is deliberately outside the "behavior" reset
          group -- resetting preferences should not silently uninstall an
          Explorer integration the user set up. */}
      <ContextMenuSetting />
      <ResetToDefaults group="behavior" />
    </div>
  )
}
