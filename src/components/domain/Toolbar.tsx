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
import { useBranches, useStashes } from '@/hooks/useGitQueries'
import { useGitMutations } from '@/hooks/useGitMutations'
import { useUiStore } from '@/stores/uiStore'
import { useActiveRepo } from '@/stores/workspaceStore'

interface ToolbarButtonProps {
  icon: ReactNode
  label: string
  badge?: string
  onClick: () => void
}

function ToolbarButton({ icon, label, badge, onClick }: ToolbarButtonProps) {
  return (
    <button
      onClick={onClick}
      title={label}
      className="flex h-8 items-center gap-[7px] rounded-md border border-transparent px-[11px] text-foreground hover:border-muted-foreground hover:bg-panel3"
    >
      <span className="relative flex flex-none">
        {icon}
        {badge && (
          <span className="absolute -right-[9px] -top-[7px] rounded-full bg-primary px-1 font-mono text-[8.5px] font-bold leading-[1.3] text-primary-foreground">
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
  const head = branches.data?.local.find((b) => b.is_head)
  const currentBranch = head?.name ?? ''

  const requireRepo = (fn: () => void) => () => {
    if (!repo) {
      toast('Open a repository first')
      return
    }
    fn()
  }

  return (
    <div className="flex h-12 flex-none items-center gap-1 border-b border-border bg-panel px-2.5">
      <ToolbarButton
        icon={<ArrowDownToLine size={16} strokeWidth={1.9} />}
        label="Fetch"
        onClick={requireRepo(() => m.fetch.mutate())}
      />
      <ToolbarButton
        icon={<ArrowDown size={16} strokeWidth={1.9} />}
        label="Pull"
        badge={head?.behind ? String(head.behind) : undefined}
        onClick={requireRepo(() => m.pull.mutate())}
      />
      <ToolbarButton
        icon={<ArrowUp size={16} strokeWidth={1.9} />}
        label="Push"
        badge={head?.ahead ? String(head.ahead) : undefined}
        onClick={requireRepo(() => m.push.mutate())}
      />

      <Separator orientation="vertical" className="mx-1.5 !h-[26px]" />

      <ToolbarButton
        icon={<GitBranch size={16} strokeWidth={1.9} />}
        label="Branch"
        onClick={requireRepo(() => {
          const name = window.prompt('New branch name:')
          if (name?.trim()) m.createBranch.mutate({ name: name.trim(), checkout: true })
        })}
      />
      <ToolbarButton
        icon={<GitMerge size={16} strokeWidth={1.9} />}
        label="Merge"
        onClick={requireRepo(() => openMerge())}
      />
      <ToolbarButton
        icon={<Archive size={16} strokeWidth={1.9} />}
        label="Stash"
        onClick={requireRepo(() => m.stashSave.mutate(undefined))}
      />
      <ToolbarButton
        icon={<ArchiveRestore size={16} strokeWidth={1.9} />}
        label="Pop"
        onClick={requireRepo(() => {
          if ((stashes.data?.length ?? 0) === 0) {
            toast('No stashes')
            return
          }
          m.stashPop.mutate(0)
        })}
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

      <GhostButton icon={<Folder size={16} strokeWidth={1.9} />} title="Show in file explorer" onClick={() => toast('Reveal in explorer (soon)')} />
      <GhostButton
        icon={
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.9" strokeLinecap="round" strokeLinejoin="round">
            <path d="M17 3l4 2v14l-4 2-9-8 9-8z" />
            <path d="M17 3v18M8 11L3 8v8l5-3z" />
          </svg>
        }
        title="Open in VS Code"
        onClick={() => toast('Open in VS Code (soon)')}
      />
      <GhostButton icon={<SquareTerminal size={16} strokeWidth={1.9} />} title="Terminal" onClick={() => toast('Terminal (soon)')} />
    </div>
  )
}
