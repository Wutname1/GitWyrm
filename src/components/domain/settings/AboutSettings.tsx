import { useEffect, useState } from 'react'
import { ChevronDown } from 'lucide-react'
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
  const updateChannel = useWorkspaceStore((s) => s.updateChannel)
  const setUpdateChannel = useWorkspaceStore((s) => s.setUpdateChannel)
  const [build, setBuild] = useState<BuildInfo | null>(null)

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
            onClick={updater.checkAndInstall}
          >
            {updater.state === 'checking'
              ? 'Checking…'
              : updater.state === 'downloading'
                ? `Downloading ${updater.version ?? ''}…`
                : 'Check for updates'}
          </Button>
          {updater.state === 'none' && (
            <span className="text-2xs text-muted-foreground">You are up to date.</span>
          )}
          {updater.state === 'error' && (
            <span className="text-2xs text-removed">Update check failed.</span>
          )}
        </div>
      </SettingRow>
    </div>
  )
}
