import { useEffect, useState } from 'react'
import { ChevronDown, Eraser, FileText, Folder, FolderOpen, X } from 'lucide-react'
import { toast } from 'sonner'
import { AiSettings } from '@/components/domain/settings/AiSettings'
import { Button } from '@/components/ui/button'
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu'
import { Input } from '@/components/ui/input'
import { Separator } from '@/components/ui/separator'
import { LogsModal } from '@/components/modals/LogsModal'
import { useUpdater } from '@/hooks/useUpdater'
import { commands, type BuildInfo } from '@/lib/bindings'
import { normalizePath } from '@/lib/paths'
import { unwrap } from '@/lib/queryKeys'
import { useUiStore } from '@/stores/uiStore'
import { useWorkspaceStore, type UpdateChannel } from '@/stores/workspaceStore'

/** Path input bound to a store setting: normalizes on blur, Browse via native dialog. */
function FolderSetting({
  value,
  placeholder,
  onCommit,
}: {
  value: string | null
  placeholder: string
  onCommit: (path: string | null) => void
}) {
  const [draft, setDraft] = useState(value ?? '')

  const commit = (raw: string) => {
    const trimmed = raw.trim()
    const normalized = trimmed ? normalizePath(trimmed) : null
    setDraft(normalized ?? '')
    onCommit(normalized)
  }

  return (
    <div className="flex gap-2">
      <Input
        value={draft}
        onChange={(e) => setDraft(e.target.value)}
        onBlur={() => commit(draft)}
        onKeyDown={(e) => e.key === 'Enter' && commit(draft)}
        placeholder={placeholder}
        className="h-8 bg-background font-mono text-xs"
      />
      <Button
        variant="secondary"
        size="sm"
        className="h-8 flex-none"
        title="Browse for folder"
        onClick={async () => {
          const { open: openDialog } = await import('@tauri-apps/plugin-dialog')
          const dir = await openDialog({ directory: true, title: 'Select folder' })
          if (typeof dir === 'string') commit(dir)
        }}
      >
        <Folder size={13} />
      </Button>
    </div>
  )
}

function SettingRow({ label, hint, children }: { label: string; hint?: string; children: React.ReactNode }) {
  return (
    <div className="flex items-start gap-6 py-3">
      <div className="w-52 flex-none">
        <div className="text-xs font-semibold text-foreground">{label}</div>
        {hint && <div className="mt-0.5 text-[10.5px] text-muted-foreground">{hint}</div>}
      </div>
      <div className="min-w-0 flex-1">{children}</div>
    </div>
  )
}

const CHANNEL_LABELS: Record<UpdateChannel, string> = {
  stable: 'Stable',
  beta: 'Beta',
}

export function SettingsView() {
  const showGraph = useUiStore((s) => s.showGraph)
  const updater = useUpdater()
  const codeFolder = useWorkspaceStore((s) => s.codeFolder)
  const setCodeFolder = useWorkspaceStore((s) => s.setCodeFolder)
  const cloneDirectory = useWorkspaceStore((s) => s.cloneDirectory)
  const setCloneDirectory = useWorkspaceStore((s) => s.setCloneDirectory)
  const updateChannel = useWorkspaceStore((s) => s.updateChannel)
  const setUpdateChannel = useWorkspaceStore((s) => s.setUpdateChannel)

  const [logsOpen, setLogsOpen] = useState(false)
  const [build, setBuild] = useState<BuildInfo | null>(null)

  useEffect(() => {
    commands
      .buildInfo()
      .then(setBuild)
      .catch(() => {})
  }, [])

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div className="flex h-10 flex-none items-center gap-2.5 border-b border-border bg-panel px-3.5">
        <span className="text-xs font-bold tracking-[.05em] text-sub">SETTINGS</span>
        <div className="flex-1" />
        <button
          onClick={showGraph}
          title="Back to graph"
          className="flex size-6 flex-none items-center justify-center rounded-[5px] border border-border bg-panel2 text-xs text-sub hover:border-muted-foreground hover:bg-panel3"
        >
          <X size={12} />
        </button>
      </div>
      <div className="min-h-0 flex-1 overflow-y-auto px-6 py-4">
        <div className="max-w-2xl">
          <h2 className="mb-1 text-sm font-bold text-foreground">General</h2>
          <Separator />
          <SettingRow label="Git executable" hint="Used for fetch, pull, push, and clone.">
            <Input defaultValue="git" className="h-8 bg-background font-mono text-xs" />
          </SettingRow>
          <SettingRow
            label="Code folder"
            hint="Scanned for repositories to quick-launch from the open dialog."
          >
            <FolderSetting
              value={codeFolder}
              placeholder="e.g. C:\code"
              onCommit={setCodeFolder}
            />
          </SettingRow>
          <SettingRow
            label="Default clone directory"
            hint="Where new clones go. Falls back to the code folder when empty."
          >
            <FolderSetting
              value={cloneDirectory}
              placeholder={codeFolder ?? 'Not set'}
              onCommit={setCloneDirectory}
            />
          </SettingRow>

          <h2 className="mb-1 mt-8 text-sm font-bold text-foreground">AI</h2>
          <Separator />
          <AiSettings />

          <h2 className="mb-1 mt-8 text-sm font-bold text-foreground">Appearance</h2>
          <Separator />
          <SettingRow label="Theme" hint="Accent and density options land in a later release.">
            <div className="text-xs text-sub">GitWyrm Dark</div>
          </SettingRow>

          <h2 className="mb-1 mt-8 text-sm font-bold text-foreground">Logs</h2>
          <Separator />
          <SettingRow
            label="Application log"
            hint="Diagnostic output written to the app log folder."
          >
            <div className="flex items-center gap-2">
              <Button
                variant="secondary"
                size="sm"
                className="h-7 gap-1.5 text-xs"
                onClick={() => setLogsOpen(true)}
              >
                <FileText size={12} />
                View logs
              </Button>
              <Button
                variant="secondary"
                size="sm"
                className="h-7 gap-1.5 text-xs"
                onClick={() => commands.openLogsFolder()}
              >
                <FolderOpen size={12} />
                Open folder
              </Button>
              <Button
                variant="secondary"
                size="sm"
                className="h-7 gap-1.5 text-xs"
                onClick={async () => {
                  try {
                    unwrap(await commands.clearLog())
                    toast('Logs cleared')
                  } catch (e) {
                    toast.error(`Could not clear logs: ${(e as Error).message}`)
                  }
                }}
              >
                <Eraser size={12} />
                Clear logs
              </Button>
            </div>
          </SettingRow>

          <h2 className="mb-1 mt-8 text-sm font-bold text-foreground">About</h2>
          <Separator />
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
                <span className="text-[11px] text-muted-foreground">You are up to date.</span>
              )}
              {updater.state === 'error' && (
                <span className="text-[11px] text-removed">Update check failed.</span>
              )}
            </div>
          </SettingRow>
        </div>
      </div>

      <LogsModal open={logsOpen} onClose={() => setLogsOpen(false)} />
    </div>
  )
}
