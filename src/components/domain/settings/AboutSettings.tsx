import { useEffect, useState } from 'react'
import { AlertTriangle, ChevronDown, RotateCcw } from 'lucide-react'
import { toast } from 'sonner'
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu'
import { Button } from '@/components/ui/button'
import { useUpdater } from '@/hooks/useUpdater'
import { commands, type BuildInfo } from '@/lib/bindings'
import { useWorkspaceStore, type UpdateChannel } from '@/stores/workspaceStore'
import { SettingRow } from './SettingRow'

const CHANNEL_LABELS: Record<UpdateChannel, string> = {
  stable: 'Stable',
  beta: 'Beta',
}

export function AboutSettings() {
  const updater = useUpdater()
  const check = useUpdater((s) => s.check)
  const updateChannel = useWorkspaceStore((s) => s.updateChannel)
  const setUpdateChannel = useWorkspaceStore((s) => s.setUpdateChannel)
  const resetAllSettings = useWorkspaceStore((s) => s.resetAllSettings)
  const restoreSettings = useWorkspaceStore((s) => s.restoreSettings)
  const [build, setBuild] = useState<BuildInfo | null>(null)

  const resetEverything = () => {
    const snapshot = resetAllSettings()
    toast.success('All settings were reset to defaults', {
      action: { label: 'Undo', onClick: () => restoreSettings(snapshot) },
      duration: 8000,
    })
  }

  useEffect(() => {
    commands
      .buildInfo()
      .then(setBuild)
      .catch(() => {})
  }, [])

  return (
    <div>
      <SettingRow label="Version">
        <div className="text-xs text-sub">
          <span className="font-mono text-foreground">{build ? `v${build.version}` : '—'}</span>
          {build && (
            <span className="ml-2 text-muted-foreground">
              built {build.build_date} · {build.git_hash}
              {build.debug ? ' · debug' : ''}
            </span>
          )}
        </div>
      </SettingRow>
      <SettingRow
        label="Update channel"
        hint="Beta receives pre-release builds. Updates are delivered from GitHub releases."
      >
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <Button variant="secondary" size="sm" className="h-7 w-28 justify-between gap-1.5 text-xs">
              {CHANNEL_LABELS[updateChannel]}
              <ChevronDown size={12} />
            </Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="start" className="w-28">
            {(Object.keys(CHANNEL_LABELS) as UpdateChannel[]).map((c) => (
              <DropdownMenuItem key={c} className="text-xs" onClick={() => setUpdateChannel(c)}>
                {CHANNEL_LABELS[c]}
              </DropdownMenuItem>
            ))}
          </DropdownMenuContent>
        </DropdownMenu>
      </SettingRow>
      <SettingRow label="Updates">
        <div className="flex items-center gap-3">
          <Button
            variant="secondary"
            size="sm"
            className="h-7 text-xs"
            disabled={updater.state === 'checking' || updater.state === 'downloading'}
            onClick={() => (updater.state === 'available' ? updater.install() : check())}
          >
            {updater.state === 'checking'
              ? 'Checking…'
              : updater.state === 'downloading'
                ? `Downloading ${updater.version ?? ''}…`
                : updater.state === 'available'
                  ? `Install ${updater.version ?? 'update'}`
                  : 'Check for updates'}
          </Button>
          {updater.state === 'none' && (
            <span className="text-2xs text-muted-foreground">You are up to date.</span>
          )}
          {updater.state === 'available' && (
            <span className="text-2xs text-accent-text">Version {updater.version} is ready to install.</span>
          )}
          {updater.state === 'error' && (
            <span className="text-2xs text-removed">Update check failed.</span>
          )}
        </div>
      </SettingRow>

      <div className="mt-8 rounded-xl border border-red-500/30 bg-red-500/[.03] p-4">
        <div className="flex items-center gap-2">
          <AlertTriangle size={15} className="text-red-400" />
          <h3 className="text-xs font-bold uppercase tracking-[.06em] text-red-400">Danger</h3>
        </div>
        <div className="mt-3 flex items-start gap-4">
          <div className="min-w-0 flex-1">
            <div className="text-xs font-semibold text-foreground">Reset all settings</div>
            <p className="mt-0.5 text-2xs leading-relaxed text-muted-foreground">
              Puts every setting on every page back to its default. Your open
              repositories, tabs, and groups are not touched. You can undo this
              right after.
            </p>
          </div>
          <Button
            variant="outline"
            size="sm"
            className="h-8 flex-none border-red-500/40 text-red-400 hover:border-red-500 hover:bg-red-500/10 hover:text-red-300"
            onClick={resetEverything}
          >
            <RotateCcw size={13} />
            Reset all settings
          </Button>
        </div>
      </div>
    </div>
  )
}
