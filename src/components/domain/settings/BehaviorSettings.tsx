import { toast } from 'sonner'
import type { TelemetryLevel } from '@/lib/bindings'
import { useWorkspaceStore } from '@/stores/workspaceStore'
import { SettingRow, SettingsGroup } from './SettingRow'
import { ContextMenuSetting } from './ContextMenuSetting'
import { ResetToDefaults } from './ResetToDefaults'

/**
 * The reporting choices, in order of how much they send.
 *
 * The wording leads with what the reader gets, because that is the honest
 * reason to leave it on: a crash nobody reports is a crash nobody fixes, and a
 * platform with no install count is one whose bugs look like nobody's problem.
 * Linux made that concrete -- a package can be mirrored or repackaged, so
 * download numbers say nothing about whether anyone is running it.
 */
const TELEMETRY_CHOICES = [
  {
    value: 'off',
    label: 'Nothing',
    detail: 'GitWyrm sends no reports and is not counted.',
    toast: 'GitWyrm will send nothing',
  },
  {
    value: 'reports',
    label: 'Crashes and a daily count',
    detail:
      'Report errors so they can be fixed, and count this install once a day so your platform is known to have users.',
    toast: 'Crashes and the daily count will be sent',
  },
  {
    value: 'full',
    label: 'Crashes, count, and speed data',
    detail: 'Also send timings and logs, which is what makes slow operations findable.',
    toast: 'Crashes, the count, and speed data will be sent',
  },
] as const satisfies ReadonlyArray<{
  value: TelemetryLevel
  label: string
  detail: string
  toast: string
}>

export function BehaviorSettings() {
  const restoreTabs = useWorkspaceStore((s) => s.restoreTabs)
  const setRestoreTabs = useWorkspaceStore((s) => s.setRestoreTabs)
  const autoFetch = useWorkspaceStore((s) => s.autoFetch)
  const setAutoFetch = useWorkspaceStore((s) => s.setAutoFetch)
  const showTips = useWorkspaceStore((s) => s.showTips)
  const setShowTips = useWorkspaceStore((s) => s.setShowTips)
  // The effective value, not the stored one: an install that has never chosen
  // stores null, and the control must show the state it is actually in.
  const telemetryLevel = useWorkspaceStore((s) => s.telemetryLevelEffective)
  const setTelemetryLevel = useWorkspaceStore((s) => s.setTelemetryLevel)
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

      <SettingsGroup
        title="Privacy & data"
        blurb="Your choice takes effect the next time GitWyrm starts."
      >
        {/* Deliberately outside the "behavior" reset group (see
            SETTINGS_DEFAULTS): resetting this page must not widen what someone
            deliberately narrowed.

            One ordered choice rather than a switch per data type. Two
            independent switches made "on" ambiguous -- what each covered
            depended on the build's version and channel -- and the reader could
            not see what they were agreeing to without knowing that rule. */}
        <SettingRow
          label="What GitWyrm reports"
          searchId="telemetry-level"
          hint="Counts are published at gitwyrm.com/stats. Files, code, paths, and repository names are never sent, at any setting."
        >
          <div className="flex flex-col gap-2">
            {TELEMETRY_CHOICES.map((choice) => (
              <label
                key={choice.value}
                className="flex cursor-pointer items-start gap-2 text-xs text-foreground"
              >
                <input
                  type="radio"
                  name="telemetry-level"
                  value={choice.value}
                  checked={telemetryLevel === choice.value}
                  onChange={() => {
                    setTelemetryLevel(choice.value)
                    // The reporters are started once at launch, so choosing
                    // changes nothing visible until a restart. Without this the
                    // radio is the only feedback, which reads as "did that do
                    // anything?" (Rule #1).
                    toast.success(`${choice.toast} from your next start.`)
                  }}
                  className="mt-0.5 size-3.5 accent-[var(--gw-accent)]"
                />
                <span className="flex flex-col gap-0.5">
                  <span>{choice.label}</span>
                  <span className="text-2xs text-muted-foreground">{choice.detail}</span>
                </span>
              </label>
            ))}
          </div>
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
