import { useEffect, useRef, useState, type ReactNode } from 'react'
import {
  BookOpen,
  ChevronDown,
  Clock,
  Columns3,
  FolderOpen,
  Loader2,
  PanelLeft,
  Plus,
  RefreshCw,
  Search,
  Settings,
} from 'lucide-react'
import { toast } from 'sonner'
import { WindowControls } from '@/components/domain/WindowControls'
import { RepositoryTabs } from '@/components/domain/RepositoryTabs'
import { WyrmExplosion, useWyrmEasterEgg } from '@/components/domain/WyrmEasterEgg'
import logoUrl from '@/assets/logo.png'
import { useOpenRepo } from '@/hooks/useRepoActions'
import { useUpdater } from '@/hooks/useUpdater'
import { cn } from '@/lib/utils'
import { useUiStore } from '@/stores/uiStore'
import {
  DEFAULT_VERTICAL_TAB_WIDTH,
  MAX_VERTICAL_TAB_WIDTH,
  MIN_VERTICAL_TAB_WIDTH,
  clampVerticalTabWidth,
  useWorkspaceStore,
} from '@/stores/workspaceStore'
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
  DropdownMenuSeparator,
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
      data-tauri-drag-region
      className="text-[0.84375rem] leading-none"
      style={{ fontFamily: 'var(--font-wordmark)', fontWeight: 600, letterSpacing: '-0.035em' }}
    >
      <span data-tauri-drag-region style={{ color: '#D7DEE7' }}>Git</span>
      <span data-tauri-drag-region style={{ color: '#2DD4A7' }}>Wyrm</span>
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
            data-tauri-drag-region
            className="flex items-center gap-[7px] outline-none"
          >
            <img
              key={bounceNonce}
              src={logoUrl}
              alt=""
              draggable={false}
              data-tauri-drag-region
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

function RepoRow({
  name,
  path,
  icon,
  onSelect,
}: {
  name: string
  path: string
  icon: ReactNode
  onSelect: () => void
}) {
  return (
    <DropdownMenuItem
      className="flex-col items-start gap-0 text-xs text-sub"
      onSelect={onSelect}
    >
      <span className="flex w-full items-center gap-2">
        {icon}
        <span className="flex-1 overflow-hidden text-ellipsis whitespace-nowrap">{name}</span>
      </span>
      <span className="w-full overflow-hidden text-ellipsis whitespace-nowrap pl-[21px] font-mono text-2xs text-muted-foreground">
        {path}
      </span>
    </DropdownMenuItem>
  )
}

function RecentRepositories({ compact = false }: { compact?: boolean }) {
  const recents = useWorkspaceStore((state) => state.recents)
  const openRepos = useWorkspaceStore((state) => state.openRepos)
  const activeRepoId = useWorkspaceStore((state) => state.activeRepoId)
  const setActiveRepo = useWorkspaceStore((state) => state.setActiveRepo)
  const openRepo = useOpenRepo()
  const [open, setOpen] = useState(false)
  const [query, setQuery] = useState('')
  const searchRef = useRef<HTMLInputElement>(null)

  // Radix focuses the first menu item on open; take focus back for the search box.
  useEffect(() => {
    if (!open) return
    const timer = setTimeout(() => searchRef.current?.focus(), 0)
    return () => clearTimeout(timer)
  }, [open])

  const search = query.trim().toLowerCase()
  const matches = (name: string, path: string) =>
    !search || name.toLowerCase().includes(search) || path.toLowerCase().includes(search)

  const openMatches = openRepos.filter((repo) => matches(repo.name, repo.path))
  const openPaths = new Set(openRepos.map((repo) => repo.path.toLowerCase()))
  const recentMatches = recents.filter(
    (repo) => !openPaths.has(repo.path.toLowerCase()) && matches(repo.name, repo.path),
  )

  return (
    <Tooltip>
      <DropdownMenu
        open={open}
        onOpenChange={(next) => {
          setOpen(next)
          if (!next) setQuery('')
        }}
      >
        <TooltipTrigger asChild>
          <DropdownMenuTrigger asChild>
            <button
              type="button"
              aria-label="Open and recent repositories"
              className={compact
                ? 'flex size-[30px] items-center justify-center rounded-[5px] border border-border bg-panel2 text-sub hover:border-muted-foreground hover:bg-panel3 hover:text-foreground'
                : 'flex items-center px-2 text-sub hover:text-foreground'}
            >
              <ChevronDown size={14} strokeWidth={2} />
            </button>
          </DropdownMenuTrigger>
        </TooltipTrigger>
        <TooltipContent>Open and recent repositories</TooltipContent>
        <DropdownMenuContent align={compact ? 'end' : 'start'} className="w-[320px] p-2">
          <div className="relative mb-1">
            <Search
              size={13}
              strokeWidth={2}
              className="pointer-events-none absolute left-2 top-1/2 -translate-y-1/2 text-muted-foreground"
            />
            <input
              ref={searchRef}
              value={query}
              onChange={(event) => setQuery(event.target.value)}
              onKeyDown={(event) => {
                // Radix menus type-ahead on printable keys; keep them in the input.
                if (event.key !== 'Escape') event.stopPropagation()
              }}
              placeholder="Search repositories"
              className="h-7 w-full rounded-[5px] border border-border bg-panel2 pl-7 pr-2 text-xs text-foreground outline-none placeholder:text-muted-foreground focus:border-muted-foreground"
            />
          </div>

          <div className="max-h-[360px] overflow-y-auto">
            <DropdownMenuLabel className="px-1.5 py-1 text-2xs font-semibold tracking-[.09em] text-muted-foreground">
              CURRENTLY OPEN
            </DropdownMenuLabel>
            {openMatches.length === 0 && (
              <div className="px-1.5 py-1.5 text-xs text-muted-foreground">
                {search ? 'No matches' : 'No repositories open'}
              </div>
            )}
            {openMatches.map((repo) => (
              <RepoRow
                key={repo.id}
                name={repo.name}
                path={repo.path}
                icon={
                  <FolderOpen
                    size={13}
                    strokeWidth={2}
                    className={repo.id === activeRepoId ? 'text-accent-text' : undefined}
                  />
                }
                onSelect={() => setActiveRepo(repo.id)}
              />
            ))}

            <DropdownMenuSeparator />

            <DropdownMenuLabel className="px-1.5 py-1 text-2xs font-semibold tracking-[.09em] text-muted-foreground">
              RECENT
            </DropdownMenuLabel>
            {recentMatches.length === 0 && (
              <div className="px-1.5 py-1.5 text-xs text-muted-foreground">
                {search ? 'No matches' : 'No recent repositories'}
              </div>
            )}
            {recentMatches.map((repo) => (
              <RepoRow
                key={repo.path}
                name={repo.name}
                path={repo.path}
                icon={<Clock size={13} strokeWidth={2} />}
                onSelect={() => openRepo.mutate(repo.path)}
              />
            ))}
          </div>
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
        ? <Loader2 size={15} strokeWidth={2} className="animate-spin text-accent-text" />
        : <Plus size={15} strokeWidth={2} />}
    </TooltipButton>
  )
}

export function TabBar() {
  const tabLayout = useWorkspaceStore((state) => state.tabLayout)
  const setTabLayout = useWorkspaceStore((state) => state.setTabLayout)
  const showSettings = useUiStore((state) => state.showSettings)

  if (tabLayout === 'vertical') {
    return (
      <div
        data-tauri-drag-region
        data-dim-on-drag
        className="flex h-9 flex-none items-center border-b border-border bg-background pl-2.5"
      >
        <div data-tauri-drag-region className="mr-4 flex items-center">
          <BrandMark />
        </div>
        <div data-tauri-drag-region className="min-w-0 flex-1" />
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
      <div data-tauri-drag-region className="flex items-center border-r border-border pr-3">
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
  const verticalTabWidth = useWorkspaceStore((state) => state.verticalTabWidth)
  const setVerticalTabWidth = useWorkspaceStore((state) => state.setVerticalTabWidth)
  const showSettings = useUiStore((state) => state.showSettings)
  const resizeStart = useRef<{ pointerId: number; x: number; width: number } | null>(null)
  const [resizing, setResizing] = useState(false)
  const compact = verticalTabWidth < 168
  const stackedControls = verticalTabWidth < 128
  const iconRail = verticalTabWidth <= 72

  const finishResize = (event: React.PointerEvent<HTMLDivElement>) => {
    const start = resizeStart.current
    if (!start || start.pointerId !== event.pointerId) return
    const width = clampVerticalTabWidth(start.width + event.clientX - start.x)
    setVerticalTabWidth(width)
    resizeStart.current = null
    setResizing(false)
    if (event.currentTarget.hasPointerCapture(event.pointerId)) {
      event.currentTarget.releasePointerCapture(event.pointerId)
    }
  }

  return (
    <aside
      className={cn(
        'relative z-20 flex min-h-0 flex-none flex-col border-r border-border bg-[color:#0d1218]',
        resizing && 'select-none',
      )}
      style={{ width: verticalTabWidth }}
    >
      <div
        className={cn(
          'flex h-[42px] flex-none items-center border-b border-border',
          iconRail ? 'justify-center px-1' : 'justify-between px-2.5 pl-3',
        )}
      >
        <span
          className={cn(
            'min-w-0 truncate font-wordmark text-2xs font-semibold text-sub',
            iconRail ? 'font-mono tracking-normal' : 'tracking-[.085em]',
          )}
          aria-label={`${openRepos.length} repositories open`}
        >
          {iconRail ? openRepos.length : compact ? `${openRepos.length} REPOS` : `REPOSITORIES · ${openRepos.length}`}
        </span>
        {!compact && (
          <div className="flex items-center gap-1.5">
            <RecentRepositories compact />
            <OpenRepositoryButton compact />
          </div>
        )}
      </div>
      <RepositoryTabs orientation="vertical" />
      <div
        className={cn(
          'flex flex-none gap-1.5 border-t border-border p-2',
          stackedControls && 'flex-col items-center',
        )}
      >
        {compact ? (
          <OpenRepositoryButton compact />
        ) : (
          <button
            type="button"
            onClick={() => useUiStore.getState().openModal('clone')}
            className="flex h-[31px] flex-1 items-center justify-center gap-1.5 rounded-[5px] border border-border bg-panel2 text-2xs text-foreground hover:border-muted-foreground hover:bg-panel3"
          >
            <Plus size={13} />
            Open a repository
          </button>
        )}
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
      <div
        role="separator"
        aria-label="Resize repository column"
        aria-orientation="vertical"
        aria-valuemin={MIN_VERTICAL_TAB_WIDTH}
        aria-valuemax={MAX_VERTICAL_TAB_WIDTH}
        aria-valuenow={verticalTabWidth}
        tabIndex={0}
        onPointerDown={(event) => {
          resizeStart.current = {
            pointerId: event.pointerId,
            x: event.clientX,
            width: verticalTabWidth,
          }
          event.currentTarget.setPointerCapture(event.pointerId)
          setResizing(true)
        }}
        onPointerMove={(event) => {
          const start = resizeStart.current
          if (!start || start.pointerId !== event.pointerId) return
          setVerticalTabWidth(start.width + event.clientX - start.x)
        }}
        onPointerUp={finishResize}
        onPointerCancel={(event) => {
          if (resizeStart.current?.pointerId !== event.pointerId) return
          resizeStart.current = null
          setResizing(false)
        }}
        onDoubleClick={() => {
          setVerticalTabWidth(DEFAULT_VERTICAL_TAB_WIDTH)
        }}
        onKeyDown={(event) => {
          if (event.key !== 'ArrowLeft' && event.key !== 'ArrowRight') return
          event.preventDefault()
          setVerticalTabWidth(verticalTabWidth + (event.key === 'ArrowLeft' ? -8 : 8))
        }}
        className={cn(
          'group absolute -right-1 top-0 z-30 h-full w-2 cursor-col-resize touch-none outline-none',
          'after:absolute after:inset-y-0 after:left-1/2 after:w-px after:-translate-x-1/2 after:bg-transparent after:transition-colors',
          'hover:after:bg-primary focus-visible:after:bg-primary',
          resizing && 'after:bg-primary',
        )}
      />
    </aside>
  )
}
