import { toast } from 'sonner'
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
  const crashReports = useWorkspaceStore((s) => s.crashReports)
  const setCrashReports = useWorkspaceStore((s) => s.setCrashReports)
  // The effective value, not the stored one: an install that has never chosen
  // stores null, and the box must show the state it is actually in.
  const usageTelemetry = useWorkspaceStore((s) => s.usageTelemetryEffective)
  const setUsageTelemetry = useWorkspaceStore((s) => s.setUsageTelemetry)
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

      <SettingsGroup title="Privacy & data" blurb="Both choices take effect the next time GitWyrm starts.">
      {/* Deliberately outside the "behavior" reset group (see
          SETTINGS_DEFAULTS): resetting this page must not turn reporting back
          on for someone who switched it off. */}
      <SettingRow
        label="Crash reports"
        searchId="crash-reports"
        hint="Send anonymous error details. Paths, keys, files, and code are never included."
      >
        <label className="flex cursor-pointer items-center gap-2 text-xs text-foreground">
          <input
            type="checkbox"
            checked={crashReports}
            onChange={(e) => {
              setCrashReports(e.target.checked)
              // The reporters are started once at launch, so flipping this
              // changes nothing visible until a restart. Without this the
              // checkbox is the only feedback, which reads as "did that do
              // anything?" (Rule #1).
              toast.success(
                e.target.checked
                  ? 'Crash reports will be sent from your next start.'
                  : 'Crash reports are off from your next start.',
              )
            }}
            className="size-3.5 accent-[var(--gw-accent)]"
          />
          Send anonymous crash reports
        </label>
      </SettingRow>
      {/* Also outside the "behavior" reset group, for the same reason as crash
          reports above. */}
      <SettingRow
        label="Usage data"
        searchId="usage-telemetry"
        hint="Send anonymous feature and speed measurements. Files and code are never included."
      >
        <label className="flex cursor-pointer items-center gap-2 text-xs text-foreground">
          <input
            type="checkbox"
            checked={usageTelemetry}
            onChange={(e) => {
              setUsageTelemetry(e.target.checked)
              // Same reasoning as crash reports: the reporters start once at
              // launch, so without this the checkbox is the only feedback
              // (Rule #1).
              toast.success(
                e.target.checked
                  ? 'Usage data will be sent from your next start.'
                  : 'Usage data is off from your next start.',
              )
            }}
            className="size-3.5 accent-[var(--gw-accent)]"
          />
          Send anonymous usage data
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
