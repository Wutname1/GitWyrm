import { type ReactNode, useState } from 'react'
import {
  ArchiveRestore,
  Archive,
  ArrowDown,
  ArrowDownToLine,
  ArrowUp,
  ChevronDown,
  Folder,
  GitBranch,
  GitMerge,
  Search,
  SquareTerminal,
} from 'lucide-react'
import { toast } from 'sonner'
import { Separator } from '@/components/ui/separator'
import { PendingIndicator } from '@/components/ui/pending-indicator'
import { TooltipButton } from '@/components/ui/tooltip'
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu'
import { detectProvider, RemoteIcon } from '@/lib/remoteProvider'
import { cn } from '@/lib/utils'
import { useBranches, useRemotes, useStashes } from '@/hooks/useGitQueries'
import { useGitMutations } from '@/hooks/useGitMutations'
import { useUiStore } from '@/stores/uiStore'
import { useActiveRepo } from '@/stores/workspaceStore'

interface ToolbarButtonProps {
  icon: ReactNode
  label: string
  badge?: string
  onClick: () => void
  disabled?: boolean
  pending?: boolean
}

function ToolbarButton({ icon, label, badge, onClick, disabled, pending }: ToolbarButtonProps) {
  return (
    <TooltipButton
      onClick={onClick}
      tooltip={label}
      disabled={disabled}
      aria-busy={pending || undefined}
      className={cn(
        'relative flex h-8 items-center gap-[7px] overflow-hidden rounded-md border border-transparent px-[11px] text-foreground transition-[border-color,background-color,color,opacity] hover:border-muted-foreground hover:bg-panel3 disabled:pointer-events-none',
        disabled && !pending && 'opacity-35',
        pending && 'wyrm-operation-active border-primary/40 bg-soft text-primary'
      )}
    >
      <span className="relative flex flex-none">
        {pending ? <PendingIndicator /> : icon}
        {badge && !pending && (
          <span className="wyrm-sync-pulse absolute -right-[9px] -top-[7px] rounded-full bg-primary px-1 font-mono text-[8.5px] font-bold leading-[1.3] text-primary-foreground">
            {badge}
          </span>
        )}
      </span>
      <span className="text-[11.5px] font-medium">{label}</span>
    </TooltipButton>
  )
}

/**
 * The current-branch pill by the search box, doubling as a hover dropdown of
 * every branch. Clicking any branch switches to it; picking a remote one lands
 * on a local branch tracking it.
 */
function BranchSwitcher() {
  const repo = useActiveRepo()
  const branches = useBranches(repo?.id ?? null)
  const remotes = useRemotes(repo?.id ?? null)
  const revealRefInGraph = useUiStore((s) => s.revealRefInGraph)
  const m = useGitMutations(repo?.id ?? null)
  const [open, setOpen] = useState(false)

  const locals = branches.data?.local ?? []
  const head = locals.find((b) => b.is_head)
  const currentBranch = head?.name ?? ''
  if (!currentBranch) return null

  const remoteList = remotes.data ?? []

  const switchTo = (name: string) => {
    if (name === currentBranch || m.checkout.isPending) return
    m.checkout.mutate(name)
    setOpen(false)
  }

  const reveal = (name: string) => {
    revealRefInGraph(name)
    setOpen(false)
  }

  return (
    <div
      className="mr-1.5"
      onMouseEnter={() => setOpen(true)}
      onMouseLeave={() => setOpen(false)}
    >
      <DropdownMenu open={open} onOpenChange={setOpen}>
        <DropdownMenuTrigger asChild>
          <button className="flex h-[30px] items-center gap-[7px] rounded-md border border-border bg-panel2 px-[11px] hover:border-muted-foreground hover:bg-panel3">
            <span className="size-2 rounded-[2px] bg-primary" />
            <span className="text-[11.5px] font-medium text-foreground">{currentBranch}</span>
            {(head?.ahead || head?.behind) ? (
              <span className="font-mono text-[10.5px] text-muted-foreground">
                {head.ahead ? `↑${head.ahead}` : ''}
                {head.ahead && head.behind ? ' ' : ''}
                {head.behind ? `↓${head.behind}` : ''}
              </span>
            ) : null}
            <ChevronDown size={13} strokeWidth={2} className="text-muted-foreground" />
          </button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align="end" className="max-h-[70vh] w-[240px]">
          <DropdownMenuLabel className="px-2 py-1 text-[9.5px] font-semibold tracking-[.09em] text-muted-foreground">
            LOCAL
          </DropdownMenuLabel>
          {locals.map((b) => {
            const isCurrent = b.name === currentBranch
            return (
              <DropdownMenuItem
                key={b.name}
                className="gap-2 text-xs"
                onClick={() => (isCurrent ? reveal(b.name) : switchTo(b.name))}
              >
                <span
                  className={cn(
                    'size-2 flex-none rounded-full',
                    isCurrent ? 'bg-primary' : 'border border-muted-foreground'
                  )}
                />
                <span
                  className={cn(
                    'flex-1 overflow-hidden text-ellipsis whitespace-nowrap',
                    isCurrent ? 'font-semibold text-foreground' : 'text-sub'
                  )}
                >
                  {b.name}
                </span>
                {(b.ahead || b.behind) ? (
                  <span className="font-mono text-[9.5px] text-muted-foreground">
                    {b.ahead ? `↑${b.ahead}` : ''}
                    {b.ahead && b.behind ? ' ' : ''}
                    {b.behind ? `↓${b.behind}` : ''}
                  </span>
                ) : null}
              </DropdownMenuItem>
            )
          })}

          {remoteList.map((r) => {
            const provider = detectProvider(r.url)
            return (
              <div key={r.name}>
                <DropdownMenuSeparator />
                <DropdownMenuLabel className="flex items-center gap-1.5 px-2 py-1 text-[9.5px] font-semibold tracking-[.09em] text-muted-foreground">
                  <RemoteIcon provider={provider} width={10} height={10} className="flex-none" />
                  {r.name.toUpperCase()}
                </DropdownMenuLabel>
                {r.branches.length === 0 ? (
                  <div className="px-2 py-1 text-[11px] text-muted-foreground">No branches</div>
                ) : (
                  r.branches.map((b) => {
                    // Already checked out locally under the same short name, so
                    // there is nothing to switch to.
                    const isCurrent = b === currentBranch
                    return (
                      <DropdownMenuItem
                        key={b}
                        className="gap-2 text-xs text-sub"
                        onClick={() =>
                          isCurrent ? reveal(b) : switchTo(`${r.name}/${b}`)
                        }
                      >
                        <span
                          className={cn(
                            'size-2 flex-none rounded-full',
                            isCurrent ? 'bg-primary' : 'border border-sub'
                          )}
                        />
                        <span
                          className={cn(
                            'flex-1 overflow-hidden text-ellipsis whitespace-nowrap font-mono',
                            isCurrent && 'font-semibold text-foreground'
                          )}
                        >
                          {b}
                        </span>
                      </DropdownMenuItem>
                    )
                  })
                )}
              </div>
            )
          })}
        </DropdownMenuContent>
      </DropdownMenu>
    </div>
  )
}

function GhostButton({ icon, tooltip, onClick }: { icon: ReactNode; tooltip: string; onClick: () => void }) {
  return (
    <TooltipButton
      onClick={onClick}
      tooltip={tooltip}
      className="flex h-[30px] w-8 items-center justify-center rounded-md border border-border bg-panel2 text-sub hover:border-muted-foreground hover:bg-panel3"
    >
      {icon}
    </TooltipButton>
  )
}

export function Toolbar() {
  const repo = useActiveRepo()
  const branches = useBranches(repo?.id ?? null)
  const stashes = useStashes(repo?.id ?? null)
  const m = useGitMutations(repo?.id ?? null)
  const openMerge = useUiStore((s) => s.openMerge)
  const openModal = useUiStore((s) => s.openModal)
  const head = branches.data?.local.find((b) => b.is_head)
  const syncAction = m.fetch.isPending
    ? 'fetch'
    : m.pull.isPending
      ? 'pull'
      : m.push.isPending
        ? 'push'
        : null
  const syncPending = syncAction !== null
  const stashAction = m.stashSave.isPending ? 'stash' : m.stashPop.isPending ? 'pop' : null
  const stashPending = stashAction !== null

  const requireRepo = (fn: () => void) => () => {
    if (!repo) {
      toast('Open a repository first')
      return
    }
    fn()
  }

  return (
    <div data-dim-on-drag className="relative flex h-12 flex-none items-center gap-1 border-b border-border bg-panel px-2.5">
      <ToolbarButton
        icon={<ArrowDownToLine size={16} strokeWidth={1.9} />}
        label={m.fetch.isPending ? 'Fetching…' : 'Fetch'}
        onClick={requireRepo(() => m.fetch.mutate())}
        disabled={syncPending}
        pending={m.fetch.isPending}
      />
      <ToolbarButton
        icon={<ArrowDown size={16} strokeWidth={1.9} />}
        label={m.pull.isPending ? 'Pulling…' : 'Pull'}
        badge={head?.behind ? String(head.behind) : undefined}
        onClick={requireRepo(() => m.pull.mutate())}
        disabled={syncPending}
        pending={m.pull.isPending}
      />
      <ToolbarButton
        icon={<ArrowUp size={16} strokeWidth={1.9} />}
        label={m.push.isPending ? 'Pushing…' : 'Push'}
        badge={head?.ahead ? String(head.ahead) : undefined}
        onClick={requireRepo(() => m.push.mutate())}
        disabled={syncPending}
        pending={m.push.isPending}
      />

      {syncAction && (
        <div
          className={cn(
            'wyrm-sync-track pointer-events-none absolute bottom-0 left-2.5 w-[224px]',
            syncAction === 'push' ? 'wyrm-sync-track-out' : 'wyrm-sync-track-in'
          )}
          aria-hidden="true"
        >
          <span />
        </div>
      )}
      <span className="sr-only" role="status" aria-live="polite">
        {syncAction === 'fetch'
          ? 'Fetching from the remote'
          : syncAction === 'pull'
            ? 'Pulling changes from the remote'
            : syncAction === 'push'
              ? 'Pushing changes to the remote'
              : ''}
      </span>

      <Separator orientation="vertical" className="mx-1.5 !h-[26px]" />

      <ToolbarButton
        icon={<GitBranch size={16} strokeWidth={1.9} />}
        label="Branch"
        onClick={requireRepo(() => openModal('newBranch'))}
      />
      <ToolbarButton
        icon={<GitMerge size={16} strokeWidth={1.9} />}
        label="Merge"
        onClick={requireRepo(() => openMerge())}
      />
      <ToolbarButton
        icon={<Archive size={16} strokeWidth={1.9} />}
        label={m.stashSave.isPending ? 'Stashing…' : 'Stash'}
        onClick={requireRepo(() => m.stashSave.mutate(undefined))}
        disabled={stashPending}
        pending={m.stashSave.isPending}
      />
      <ToolbarButton
        icon={<ArchiveRestore size={16} strokeWidth={1.9} />}
        label={m.stashPop.isPending ? 'Restoring…' : 'Pop'}
        onClick={requireRepo(() => {
          if ((stashes.data?.length ?? 0) === 0) {
            toast('No stashes')
            return
          }
          m.stashPop.mutate(0)
        })}
        disabled={stashPending}
        pending={m.stashPop.isPending}
      />

      <div className="flex-1" />

      <BranchSwitcher />

      <button
        onClick={() => toast('Command palette · Ctrl+K')}
        className="flex h-[30px] min-w-[190px] items-center gap-[7px] rounded-md border border-border bg-panel2 px-2.5 text-muted-foreground hover:border-muted-foreground hover:bg-panel3"
      >
        <Search size={14} strokeWidth={1.9} />
        <span className="text-[11.5px]">Search commits, files…</span>
        <span className="ml-auto rounded border border-border px-1 font-mono text-[10px]">Ctrl+K</span>
      </button>

      <GhostButton
        icon={<Folder size={16} strokeWidth={1.9} />}
        tooltip="Show in file explorer"
        onClick={requireRepo(() => m.revealInFileManager.mutate())}
      />
      <GhostButton
        icon={
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.9" strokeLinecap="round" strokeLinejoin="round">
            <path d="M17 3l4 2v14l-4 2-9-8 9-8z" />
            <path d="M17 3v18M8 11L3 8v8l5-3z" />
          </svg>
        }
        tooltip="Open in VS Code"
        onClick={requireRepo(() => m.openInEditor.mutate())}
      />
      <GhostButton
        icon={<SquareTerminal size={16} strokeWidth={1.9} />}
        tooltip="Open in terminal"
        onClick={requireRepo(() => m.openInTerminal.mutate())}
      />
    </div>
  )
}
