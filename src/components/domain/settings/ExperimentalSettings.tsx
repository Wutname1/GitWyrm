import { ResetToDefaults } from './ResetToDefaults'
import { SettingRow, SettingsGroup } from './SettingRow'
import { useWorkspaceStore } from '@/stores/workspaceStore'

/**
 * Opt-in home for features that are not finished.
 *
 * Exists so a half-built surface can ship to the people who want to try it
 * without becoming the default experience for everyone else. Anything gated
 * here should be genuinely usable but not yet trustworthy enough to be the
 * thing a new user meets first.
 */
export function ExperimentalSettings() {
  return (
    <div>
      <SettingsGroup
        title="Unfinished features"
        blurb="Try work in progress. These can change or break between versions."
      >
        <AgentDeskSetting />
      </SettingsGroup>
      <ResetToDefaults group="experimental" label="Turn off all experimental features" />
    </div>
  )
}

/** Switches the Desk window between Spec Desk (default) and the AI harness. */
function AgentDeskSetting() {
  const enabled = useWorkspaceStore((s) => s.experimentalFeatures)
  const setEnabled = useWorkspaceStore((s) => s.setExperimentalFeatures)

  return (
    <SettingRow
      label="Agent Desk (AI harness)"
      searchId="experimental-agent-desk"
      hint="Replaces Spec Desk with the new AI agent workspace. Still being built, so expect rough edges. Turn it off to go back to Spec Desk."
    >
      <label className="flex cursor-pointer items-center gap-2 text-xs text-foreground">
        <input
          type="checkbox"
          checked={enabled}
          onChange={(e) => setEnabled(e.target.checked)}
          className="size-3.5 accent-[var(--gw-accent)]"
        />
        Use Agent Desk
      </label>
    </SettingRow>
  )
}
