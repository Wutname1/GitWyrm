import { toast } from 'sonner'
import { useWorkspaceStore } from '@/stores/workspaceStore'
import { SettingRow } from './SettingRow'
import { ContextMenuSetting } from './ContextMenuSetting'
import { ResetToDefaults } from './ResetToDefaults'

export function BehaviorSettings() {
  const restoreTabs = useWorkspaceStore((s) => s.restoreTabs)
  const setRestoreTabs = useWorkspaceStore((s) => s.setRestoreTabs)
  const autoFetch = useWorkspaceStore((s) => s.autoFetch)
  const setAutoFetch = useWorkspaceStore((s) => s.setAutoFetch)
  const crashReports = useWorkspaceStore((s) => s.crashReports)
  const setCrashReports = useWorkspaceStore((s) => s.setCrashReports)

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
      {/* Deliberately outside the "behavior" reset group (see
          SETTINGS_DEFAULTS): resetting this page must not turn reporting back
          on for someone who switched it off. */}
      <SettingRow
        label="Crash reports"
        searchId="crash-reports"
        hint="When GitWyrm hits an error, it sends a report so the problem can be fixed. Reports say what went wrong and where in the code -- never your files, your code, or your commit history. Paths, branch names, and keys are removed before anything is sent. Takes effect next time you start GitWyrm."
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
      {/* Registry-backed, so it is deliberately outside the "behavior" reset
          group -- resetting preferences should not silently uninstall an
          Explorer integration the user set up. */}
      <ContextMenuSetting />
      <ResetToDefaults group="behavior" />
    </div>
  )
}
