import {
  BookOpen,
  ChevronDown,
  Clock,
  Columns3,
  Loader2,
  PanelLeft,
  Plus,
  RefreshCw,
  Settings,
} from 'lucide-react'
import { toast } from 'sonner'
import { WindowControls } from '@/components/domain/WindowControls'
import { RepositoryTabs } from '@/components/domain/RepositoryTabs'
import { WyrmExplosion, useWyrmEasterEgg } from '@/components/domain/WyrmEasterEgg'
import logoUrl from '@/assets/logo.png'
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
import {
  Tooltip,
  TooltipButton,
  TooltipContent,
  TooltipTrigger,
} from '@/components/ui/tooltip'

const DOCS_URL = 'https://github.com/Wutname1/GitWyrm'

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
  const showSettings = useUiStore((state) => state.showSettings)
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

function RecentRepositories() {
  const recents = useWorkspaceStore((state) => state.recents)
  const openRepo = useOpenRepo()

  return (
    <Tooltip>
      <DropdownMenu>
        <TooltipTrigger asChild>
          <DropdownMenuTrigger asChild>
            <button
              aria-label="Recent repositories"
              className="flex items-center px-2 text-sub hover:text-foreground"
            >
              <ChevronDown size={14} strokeWidth={2} />
            </button>
          </DropdownMenuTrigger>
        </TooltipTrigger>
        <TooltipContent>Recent repositories</TooltipContent>
        <DropdownMenuContent align="start" className="w-[300px] p-2">
          <DropdownMenuLabel className="px-1.5 py-1 text-[9.5px] font-semibold tracking-[.09em] text-muted-foreground">
            RECENT
          </DropdownMenuLabel>
          {recents.length === 0 && (
            <div className="px-1.5 py-2 text-xs text-muted-foreground">No recent repositories</div>
          )}
          {recents.map((repo) => (
            <DropdownMenuItem
              key={repo.path}
              className="gap-2 text-xs text-sub"
              onClick={() => openRepo.mutate(repo.path)}
            >
              <Clock size={13} strokeWidth={2} />
              <span className="flex-1 overflow-hidden text-ellipsis whitespace-nowrap">{repo.name}</span>
              <span className="font-mono text-[9px] text-muted-foreground">{repo.path}</span>
            </DropdownMenuItem>
          ))}
        </DropdownMenuContent>
      </DropdownMenu>
    </Tooltip>
  )
}

function OpenRepositoryButton({ compact = false }: { compact?: boolean }) {
  const openModal = useUiStore((state) => state.openModal)
  const openRepo = useOpenRepo()
  return (
    <TooltipButton
      onClick={() => openModal('clone')}
      className={compact
        ? 'flex size-[30px] items-center justify-center rounded-[5px] border border-border bg-panel2 text-sub hover:border-muted-foreground hover:bg-panel3 hover:text-foreground'
        : 'flex items-center px-2 text-sub hover:text-foreground'}
      tooltip="Open or clone a repository"
      disabled={openRepo.isPending}
    >
      {openRepo.isPending
        ? <Loader2 size={15} strokeWidth={2} className="animate-spin text-primary" />
        : <Plus size={15} strokeWidth={2} />}
    </TooltipButton>
  )
}

export function TabBar() {
  const tabLayout = useWorkspaceStore((state) => state.tabLayout)
  const activeRepoId = useWorkspaceStore((state) => state.activeRepoId)
  const activeRepo = useWorkspaceStore((state) => state.openRepos.find((repo) => repo.id === activeRepoId))
  const setTabLayout = useWorkspaceStore((state) => state.setTabLayout)
  const showSettings = useUiStore((state) => state.showSettings)

  if (tabLayout === 'vertical') {
    return (
      <div
        data-tauri-drag-region
        data-dim-on-drag
        className="flex h-9 flex-none items-center border-b border-border bg-background pl-2.5"
      >
        <div className="mr-4 flex items-center">
          <BrandMark />
        </div>
        <span className="min-w-0 flex-1 overflow-hidden text-ellipsis whitespace-nowrap font-mono text-[9.5px] text-muted-foreground">
          {activeRepo?.path ?? ''}
        </span>
        <WindowControls />
      </div>
    )
  }

  return (
    <div
      data-tauri-drag-region
      data-dim-on-drag
      className="flex h-9 flex-none items-stretch gap-0.5 border-b border-border bg-background pl-2.5"
    >
      <div className="mr-1 flex items-center border-r border-border pr-3">
        <BrandMark />
      </div>
      <RepositoryTabs orientation="horizontal" />
      <RecentRepositories />
      <OpenRepositoryButton />
      <div data-tauri-drag-region className="min-w-3 flex-1" />
      <TooltipButton
        onClick={() => {
          setTabLayout('vertical')
          toast.success('Repository tabs moved to the left side')
        }}
        className="flex items-center px-2 text-sub hover:text-foreground"
        tooltip="Use vertical tabs"
      >
        <PanelLeft size={15} strokeWidth={1.9} />
      </TooltipButton>
      <TooltipButton
        onClick={() => showSettings()}
        className="flex items-center px-2 text-sub hover:text-foreground"
        tooltip="Settings"
      >
        <Settings size={15} strokeWidth={1.9} />
      </TooltipButton>
      <WindowControls />
    </div>
  )
}

export function VerticalTabRail() {
  const openRepos = useWorkspaceStore((state) => state.openRepos)
  const setTabLayout = useWorkspaceStore((state) => state.setTabLayout)
  const showSettings = useUiStore((state) => state.showSettings)

  return (
    <aside className="flex w-[248px] flex-none flex-col border-r border-border bg-[color:#0d1218]">
      <div className="flex h-[42px] flex-none items-center justify-between border-b border-border px-2.5 pl-3">
        <span className="font-wordmark text-[9px] font-semibold tracking-[.085em] text-sub">
          REPOSITORIES · {openRepos.length}
        </span>
        <OpenRepositoryButton compact />
      </div>
      <RepositoryTabs orientation="vertical" />
      <div className="flex flex-none gap-1.5 border-t border-border p-2">
        <button
          type="button"
          onClick={() => useUiStore.getState().openModal('clone')}
          className="flex h-[31px] flex-1 items-center justify-center gap-1.5 rounded-[5px] border border-border bg-panel2 text-[11px] text-foreground hover:border-muted-foreground hover:bg-panel3"
        >
          <Plus size={13} />
          Open a repository
        </button>
        <TooltipButton
          onClick={() => {
            setTabLayout('horizontal')
            toast.success('Repository tabs moved to the top')
          }}
          className="flex size-[31px] items-center justify-center rounded-[5px] border border-border bg-panel2 text-sub hover:border-muted-foreground hover:bg-panel3 hover:text-foreground"
          tooltip="Use top tabs"
        >
          <Columns3 size={14} />
        </TooltipButton>
        <TooltipButton
          onClick={() => showSettings()}
          className="flex size-[31px] items-center justify-center rounded-[5px] border border-border bg-panel2 text-sub hover:border-muted-foreground hover:bg-panel3 hover:text-foreground"
          tooltip="Settings"
        >
          <Settings size={14} />
        </TooltipButton>
      </div>
    </aside>
  )
}
