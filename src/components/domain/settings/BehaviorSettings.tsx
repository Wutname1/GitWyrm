import { useWorkspaceStore } from '@/stores/workspaceStore'
import { SettingRow, SettingsGroup } from './SettingRow'
import { ContextMenuSetting } from './ContextMenuSetting'
import { ResetToDefaults } from './ResetToDefaults'

export function BehaviorSettings() {
  const restoreTabs = useWorkspaceStore((s) => s.restoreTabs)
  const setRestoreTabs = useWorkspaceStore((s) => s.setRestoreTabs)
  const autoFetch = useWorkspaceStore((s) => s.autoFetch)
  const setAutoFetch = useWorkspaceStore((s) => s.setAutoFetch)
  const showTips = useWorkspaceStore((s) => s.showTips)
  const setShowTips = useWorkspaceStore((s) => s.setShowTips)
  const discardResetsSubmodules = useWorkspaceStore((s) => s.discardResetsSubmodules)
  const setDiscardResetsSubmodules = useWorkspaceStore((s) => s.setDiscardResetsSubmodules)

  return (
    <div>
      <SettingsGroup title="Opening GitWyrm">
        <SettingRow
          label="On startup"
          searchId="restore-tabs"
          hint="Bring back the repositories that were open last time."
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
          label="Tips"
          searchId="show-tips"
          hint="Show short explanations beside unfamiliar features."
        >
          <label className="flex cursor-pointer items-center gap-2 text-xs text-foreground">
            <input
              type="checkbox"
              checked={showTips}
              onChange={(e) => setShowTips(e.target.checked)}
              className="size-3.5 accent-[var(--gw-accent)]"
            />
            Explain features to me
          </label>
        </SettingRow>
      </SettingsGroup>

      <SettingsGroup title="Background updates">
        <SettingRow
          label="Remote changes"
          searchId="auto-fetch"
          hint="Check for new remote work without changing your files."
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
      </SettingsGroup>

      <SettingsGroup title="Safety defaults">
        <SettingRow
          label="Discarding everything"
          searchId="discard-submodules"
          hint="Choose whether moved submodules start selected in the discard warning."
        >
          <label className="flex cursor-pointer items-center gap-2 text-xs text-foreground">
            <input
              type="checkbox"
              checked={discardResetsSubmodules}
              onChange={(e) => setDiscardResetsSubmodules(e.target.checked)}
              className="size-3.5 accent-[var(--gw-accent)]"
            />
            Also put moved submodules back
          </label>
        </SettingRow>
      </SettingsGroup>

      <SettingsGroup title="Windows integration">
      {/* Registry-backed, so it is deliberately outside the "behavior" reset
          group -- resetting preferences should not silently uninstall an
          Explorer integration the user set up. */}
      <ContextMenuSetting />
      </SettingsGroup>
      <ResetToDefaults group="behavior" />
    </div>
  )
}
