import type { ReactNode } from 'react'
import {
  ArchiveRestore,
  Archive,
  ArrowDown,
  ArrowDownToLine,
  ArrowUp,
  Folder,
  GitBranch,
  GitMerge,
  Search,
  SquareTerminal,
} from 'lucide-react'
import { toast } from 'sonner'
import { Separator } from '@/components/ui/separator'
import { PendingIndicator } from '@/components/ui/pending-indicator'
import { cn } from '@/lib/utils'
import { useBranches, useStashes } from '@/hooks/useGitQueries'
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
    <button
      onClick={onClick}
      title={label}
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
    </button>
  )
}

function GhostButton({ icon, title, onClick }: { icon: ReactNode; title: string; onClick: () => void }) {
  return (
    <button
      onClick={onClick}
      title={title}
      className="flex h-[30px] w-8 items-center justify-center rounded-md border border-border bg-panel2 text-sub hover:border-muted-foreground hover:bg-panel3"
    >
      {icon}
    </button>
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
  const currentBranch = head?.name ?? ''
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
    <div className="relative flex h-12 flex-none items-center gap-1 border-b border-border bg-panel px-2.5">
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

      {currentBranch && (
        <div className="mr-1.5 flex h-[30px] items-center gap-[7px] rounded-md border border-border bg-panel2 px-[11px]">
          <span className="size-2 rounded-[2px] bg-primary" />
          <span className="text-[11.5px] font-medium text-foreground">{currentBranch}</span>
          {(head?.ahead || head?.behind) ? (
            <span className="font-mono text-[10.5px] text-muted-foreground">
              {head.ahead ? `↑${head.ahead}` : ''}
              {head.ahead && head.behind ? ' ' : ''}
              {head.behind ? `↓${head.behind}` : ''}
            </span>
          ) : null}
        </div>
      )}

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
        title="Show in file explorer"
        onClick={requireRepo(() => m.revealInFileManager.mutate())}
      />
      <GhostButton
        icon={
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.9" strokeLinecap="round" strokeLinejoin="round">
            <path d="M17 3l4 2v14l-4 2-9-8 9-8z" />
            <path d="M17 3v18M8 11L3 8v8l5-3z" />
          </svg>
        }
        title="Open in VS Code"
        onClick={requireRepo(() => m.openInEditor.mutate())}
      />
      <GhostButton
        icon={<SquareTerminal size={16} strokeWidth={1.9} />}
        title="Open in terminal"
        onClick={requireRepo(() => m.openInTerminal.mutate())}
      />
    </div>
  )
}
