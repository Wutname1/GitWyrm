import { ChevronDown, Clock, Loader2, Plus, Settings, X } from 'lucide-react'
import { cn } from '@/lib/utils'
import { WindowControls } from '@/components/domain/WindowControls'
import logoUrl from '@/assets/logo.png'
import { commands } from '@/lib/bindings'
import { useOpenRepo } from '@/hooks/useRepoActions'
import { useUiStore } from '@/stores/uiStore'
import { useWorkspaceStore } from '@/stores/workspaceStore'
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu'

const TAB_DOTS = ['var(--gw-accent)', '#38bdf8', '#f59e0b', '#e06b9a', '#a78bfa']

function WyrmLogo() {
  return <img src={logoUrl} alt="GitWyrm" className="size-[18px] flex-none" draggable={false} />
}

function Wordmark() {
  return (
    <span
      data-tauri-drag-region
      className="text-[13.5px] leading-none"
      style={{ fontFamily: 'var(--font-wordmark)', fontWeight: 600, letterSpacing: '-0.035em' }}
    >
      <span style={{ color: '#D7DEE7' }}>Git</span>
      <span style={{ color: '#2DD4A7' }}>Wyrm</span>
    </span>
  )
}

export function TabBar() {
  const openRepos = useWorkspaceStore((s) => s.openRepos)
  const activeRepoId = useWorkspaceStore((s) => s.activeRepoId)
  const setActiveRepo = useWorkspaceStore((s) => s.setActiveRepo)
  const removeRepo = useWorkspaceStore((s) => s.removeRepo)
  const recents = useWorkspaceStore((s) => s.recents)
  const showSettings = useUiStore((s) => s.showSettings)
  const openModal = useUiStore((s) => s.openModal)
  const openRepo = useOpenRepo()

  return (
    <div
      data-tauri-drag-region
      className="flex h-9 flex-none items-stretch gap-0.5 border-b border-border bg-background pl-2.5"
    >
      <div
        data-tauri-drag-region
        className="mr-1 flex items-center gap-[7px] border-r border-border pr-3"
      >
        <WyrmLogo />
        <Wordmark />
      </div>

      {openRepos.map((r, i) => (
        <div
          key={r.id}
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
          <span className="whitespace-nowrap">{r.name}</span>
          {r.head_branch && (
            <span className="font-mono text-[10px] text-muted-foreground">{r.head_branch}</span>
          )}
          <button
            onClick={(e) => {
              e.stopPropagation()
              commands.closeRepo(r.id)
              removeRepo(r.id)
            }}
            className="ml-0.5 flex-none rounded p-0.5 text-muted-foreground opacity-0 hover:bg-panel3 hover:text-foreground group-hover:opacity-100"
            title="Close repository"
          >
            <X size={11} />
          </button>
        </div>
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
    </div>
  )
}
