import { useEffect, useState } from 'react'
import { AlertTriangle, ChevronDown, ExternalLink, RotateCcw } from 'lucide-react'
import { toast } from 'sonner'
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu'
import { Button } from '@/components/ui/button'
import { useUpdater } from '@/hooks/useUpdater'
import { openWebUrl } from '@/lib/remoteWeb'
import { commands, type BuildInfo } from '@/lib/bindings'
import { useWorkspaceStore, type UpdateChannel } from '@/stores/workspaceStore'
import { cn } from '@/lib/utils'
import { SettingRow, SettingsGroup, useRevealHighlight } from './SettingRow'

const CHANNEL_LABELS: Record<UpdateChannel, string> = {
  stable: 'Stable',
  beta: 'Beta',
}

export function AboutSettings() {
  const updater = useUpdater()
  const check = useUpdater((s) => s.check)
  const updateChannel = useWorkspaceStore((s) => s.updateChannel)
  const setUpdateChannel = useWorkspaceStore((s) => s.setUpdateChannel)
  const autoUpdate = useWorkspaceStore((s) => s.autoUpdate)
  const setAutoUpdate = useWorkspaceStore((s) => s.setAutoUpdate)
  const resetAllSettings = useWorkspaceStore((s) => s.resetAllSettings)
  const restoreSettings = useWorkspaceStore((s) => s.restoreSettings)
  const [build, setBuild] = useState<BuildInfo | null>(null)
  const resetReveal = useRevealHighlight('reset-all')

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
      <SettingsGroup title="Version & updates">
      <SettingRow label="Version" searchId="version" hint="The build installed on this computer.">
        <div className="text-xs text-sub">
          <span className="font-mono text-foreground">
            {build ? `v${build.version}${build.arch === 'aarch64' ? ' (ARM)' : ''}` : '—'}
          </span>
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
        searchId="update-channel"
        hint="Choose stable releases or early preview builds."
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
      <SettingRow
        label="Automatic updates"
        searchId="auto-update"
        hint="Install new versions at startup instead of asking first."
      >
        <label className="flex cursor-pointer items-center gap-2 text-xs text-foreground">
          <input
            type="checkbox"
            checked={autoUpdate}
            onChange={(e) => setAutoUpdate(e.target.checked)}
            className="size-3.5 accent-[var(--gw-accent)]"
          />
          Install updates automatically
        </label>
      </SettingRow>
      <SettingRow label="Check now" searchId="updates" hint="Look for a newer version of GitWyrm.">
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
      </SettingsGroup>

      <SettingsGroup title="Legal">
        <SettingRow
          label="Privacy and terms"
          searchId="legal-links"
          hint="What GitWyrm does with your data, and the terms you use it under. Both open on gitwyrm.com."
        >
          <div className="flex flex-wrap items-center gap-2">
            <Button
              variant="secondary"
              size="sm"
              className="h-7 gap-1.5 text-xs"
              onClick={() => openWebUrl('https://gitwyrm.com/privacy', 'the privacy policy')}
            >
              <ExternalLink size={12} />
              Privacy policy
            </Button>
            <Button
              variant="secondary"
              size="sm"
              className="h-7 gap-1.5 text-xs"
              onClick={() => openWebUrl('https://gitwyrm.com/terms', 'the terms of service')}
            >
              <ExternalLink size={12} />
              Terms of service
            </Button>
          </div>
        </SettingRow>
      </SettingsGroup>

      <div
        ref={resetReveal.ref}
        className={cn(
          'mt-8 scroll-mt-6 rounded-xl border border-red-500/30 bg-red-500/[.03] p-4 transition-shadow duration-500',
          resetReveal.flash && 'ring-1 ring-primary/50'
        )}
      >
        <div className="flex items-center gap-2">
          <AlertTriangle size={15} className="text-red-400" />
          <h3 className="text-xs font-semibold text-red-400">Restore app settings</h3>
        </div>
        <div className="mt-3 flex items-start gap-4">
          <div className="min-w-0 flex-1">
            <div className="text-xs font-semibold text-foreground">Reset all settings</div>
            <p className="mt-0.5 text-2xs leading-relaxed text-muted-foreground">
              Restore every page to its default choices. Your repositories and commits stay safe.
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
