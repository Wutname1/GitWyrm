import { useEffect, useState } from 'react'
import { BookOpen, ChevronDown, Clock, Loader2, Pencil, Plus, RefreshCw, Settings, X } from 'lucide-react'
import { cn } from '@/lib/utils'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import type { RepoInfo } from '@/lib/bindings'
import { WindowControls } from '@/components/domain/WindowControls'
import { WyrmExplosion, useWyrmEasterEgg } from '@/components/domain/WyrmEasterEgg'
import logoUrl from '@/assets/logo.png'
import { commands } from '@/lib/bindings'
import { useOpenRepo } from '@/hooks/useRepoActions'
import { useUpdater } from '@/hooks/useUpdater'
import { useUiStore } from '@/stores/uiStore'
import { useWorkspaceStore } from '@/stores/workspaceStore'
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuSeparator,
  ContextMenuTrigger,
} from '@/components/ui/context-menu'
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu'

const DOCS_URL = 'https://github.com/Wutname1/GitWyrm'

const TAB_DOTS = ['var(--gw-accent)', '#38bdf8', '#f59e0b', '#e06b9a', '#a78bfa']

function Wordmark() {
  return (
    <span
      className="text-[13.5px] leading-none"
      style={{ fontFamily: 'var(--font-wordmark)', fontWeight: 600, letterSpacing: '-0.035em' }}
    >
      <span style={{ color: '#D7DEE7' }}>Git</span>
      <span style={{ color: '#2DD4A7' }}>Wyrm</span>
    </span>
  )
}

function openDocs() {
  void import('@tauri-apps/plugin-opener').then(({ openUrl }) => openUrl(DOCS_URL))
}

function BrandMark() {
  const showSettings = useUiStore((s) => s.showSettings)
  const { checkAndInstall } = useUpdater()
  const { onLogoClick, bounceNonce, blast } = useWyrmEasterEgg()

  return (
    <>
      <ContextMenu>
        <ContextMenuTrigger asChild>
          <button
            type="button"
            onClick={onLogoClick}
            aria-label="GitWyrm"
            className="flex items-center gap-[7px] outline-none"
          >
            <img
              key={bounceNonce}
              src={logoUrl}
              alt=""
              draggable={false}
              className="size-[18px] flex-none wyrm-spring"
            />
            <Wordmark />
          </button>
        </ContextMenuTrigger>
        <ContextMenuContent className="w-48">
          <ContextMenuItem onSelect={openDocs}>
            <BookOpen size={13} strokeWidth={2} />
            Open docs
          </ContextMenuItem>
          <ContextMenuItem onSelect={() => checkAndInstall()}>
            <RefreshCw size={13} strokeWidth={2} />
            Check for updates
          </ContextMenuItem>
          <ContextMenuSeparator />
          <ContextMenuItem onSelect={() => showSettings()}>
            <Settings size={13} strokeWidth={2} />
            Settings
          </ContextMenuItem>
        </ContextMenuContent>
      </ContextMenu>
      <WyrmExplosion blast={blast} />
    </>
  )
}

/** Renames a tab. An empty name clears the alias, restoring the folder name. */
function RenameTabDialog({
  repo,
  currentName,
  onOpenChange,
  onConfirm,
}: {
  repo: RepoInfo
  currentName: string
  onOpenChange: (open: boolean) => void
  onConfirm: (alias: string) => void
}) {
  const [value, setValue] = useState(currentName)

  useEffect(() => {
    setValue(currentName)
  }, [currentName])

  const submit = () => onConfirm(value)

  return (
    <Dialog open onOpenChange={onOpenChange}>
      <DialogContent className="gap-0 p-0 sm:max-w-md" aria-describedby={undefined}>
        <DialogHeader className="border-b border-border px-4 pb-3 pt-4">
          <DialogTitle className="flex items-center gap-2 text-sm">
            <Pencil size={15} strokeWidth={1.9} />
            Rename tab
          </DialogTitle>
        </DialogHeader>

        <div className="grid gap-1.5 px-4 py-4">
          <label className="text-[11px] font-semibold text-sub">Tab name</label>
          <Input
            value={value}
            onChange={(e) => setValue(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === 'Enter') submit()
            }}
            placeholder={repo.name}
            className="h-auto bg-background py-1.5 text-xs"
            autoFocus
          />
          <p className="text-[10.5px] text-muted-foreground">Leave blank to use the folder name.</p>
        </div>

        <DialogFooter className="flex justify-end gap-2 border-t border-border px-4 py-3">
          <Button variant="secondary" size="sm" onClick={() => onOpenChange(false)}>
            Cancel
          </Button>
          <Button size="sm" onClick={submit}>
            Save
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}

export function TabBar() {
  const openRepos = useWorkspaceStore((s) => s.openRepos)
  const activeRepoId = useWorkspaceStore((s) => s.activeRepoId)
  const setActiveRepo = useWorkspaceStore((s) => s.setActiveRepo)
  const removeRepo = useWorkspaceStore((s) => s.removeRepo)
  const recents = useWorkspaceStore((s) => s.recents)
  const tabAliases = useWorkspaceStore((s) => s.tabAliases)
  const setTabAlias = useWorkspaceStore((s) => s.setTabAlias)
  const showSettings = useUiStore((s) => s.showSettings)
  const openModal = useUiStore((s) => s.openModal)
  const openRepo = useOpenRepo()
  const [renaming, setRenaming] = useState<RepoInfo | null>(null)

  const closeTab = (id: string) => {
    commands.closeRepo(id)
    removeRepo(id)
  }
  const closeOthers = (keepId: string) => {
    for (const r of openRepos) if (r.id !== keepId) closeTab(r.id)
  }
  const closeToRight = (id: string) => {
    const idx = openRepos.findIndex((r) => r.id === id)
    if (idx === -1) return
    for (const r of openRepos.slice(idx + 1)) closeTab(r.id)
  }
  const tabName = (r: RepoInfo) => tabAliases[r.path] ?? r.name

  return (
    <div
      data-tauri-drag-region
      data-dim-on-drag
      className="flex h-9 flex-none items-stretch gap-0.5 border-b border-border bg-background pl-2.5"
    >
      <div className="mr-1 flex items-center border-r border-border pr-3">
        <BrandMark />
      </div>

      {openRepos.map((r, i) => (
        <ContextMenu key={r.id}>
          <ContextMenuTrigger asChild>
            <div
              onClick={() => setActiveRepo(r.id)}
              className={cn(
                'group flex cursor-pointer items-center gap-[7px] border-r border-border border-t-2 px-3 text-xs',
                r.id === activeRepoId
                  ? 'border-t-primary bg-panel font-semibold text-foreground'
                  : 'border-t-transparent text-sub'
              )}
            >
              <span
                className="size-[7px] flex-none rounded-[2px]"
                style={{ background: TAB_DOTS[i % TAB_DOTS.length] }}
              />
              <span className="whitespace-nowrap">{tabName(r)}</span>
              <button
                onClick={(e) => {
                  e.stopPropagation()
                  closeTab(r.id)
                }}
                className="ml-0.5 flex-none rounded p-0.5 text-muted-foreground opacity-0 hover:bg-panel3 hover:text-foreground group-hover:opacity-100"
                title="Close repository"
              >
                <X size={11} />
              </button>
            </div>
          </ContextMenuTrigger>
          <ContextMenuContent className="w-48">
            <ContextMenuItem onSelect={() => setRenaming(r)}>
              <Pencil size={13} strokeWidth={2} />
              Rename tab
            </ContextMenuItem>
            <ContextMenuSeparator />
            <ContextMenuItem onSelect={() => closeTab(r.id)}>
              <X size={13} strokeWidth={2} />
              Close tab
            </ContextMenuItem>
            <ContextMenuItem
              disabled={openRepos.length <= 1}
              onSelect={() => closeOthers(r.id)}
            >
              Close other tabs
            </ContextMenuItem>
            <ContextMenuItem
              disabled={i === openRepos.length - 1}
              onSelect={() => closeToRight(r.id)}
            >
              Close tabs to the right
            </ContextMenuItem>
          </ContextMenuContent>
        </ContextMenu>
      ))}

      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <button className="flex items-center px-2 text-sub hover:text-foreground" title="Recent repositories">
            <ChevronDown size={14} strokeWidth={2} />
          </button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align="start" className="w-[300px] p-2">
          <DropdownMenuLabel className="px-1.5 py-1 text-[9.5px] font-semibold tracking-[.09em] text-muted-foreground">
            RECENT
          </DropdownMenuLabel>
          {recents.length === 0 && (
            <div className="px-1.5 py-2 text-xs text-muted-foreground">No recent repositories</div>
          )}
          {recents.map((r) => (
            <DropdownMenuItem
              key={r.path}
              className="gap-2 text-xs text-sub"
              onClick={() => openRepo.mutate(r.path)}
            >
              <Clock size={13} strokeWidth={2} />
              <span className="flex-1 overflow-hidden text-ellipsis whitespace-nowrap">{r.name}</span>
              <span className="font-mono text-[9px] text-muted-foreground">{r.path}</span>
            </DropdownMenuItem>
          ))}
        </DropdownMenuContent>
      </DropdownMenu>

      <button
        onClick={() => openModal('clone')}
        className="flex items-center px-2 text-sub hover:text-foreground"
        title="Open or clone a repository"
        disabled={openRepo.isPending}
      >
        {openRepo.isPending ? (
          <Loader2 size={15} strokeWidth={2} className="animate-spin text-primary" />
        ) : (
          <Plus size={15} strokeWidth={2} />
        )}
      </button>

      <div data-tauri-drag-region className="flex-1" />

      <button
        onClick={() => showSettings()}
        className="flex items-center px-2 text-sub hover:text-foreground"
        title="Settings"
      >
        <Settings size={15} strokeWidth={1.9} />
      </button>

      <WindowControls />

      {renaming && (
        <RenameTabDialog
          repo={renaming}
          currentName={tabAliases[renaming.path] ?? ''}
          onOpenChange={(open) => {
            if (!open) setRenaming(null)
          }}
          onConfirm={(alias) => {
            setTabAlias(renaming.path, alias)
            setRenaming(null)
          }}
        />
      )}
    </div>
  )
}
