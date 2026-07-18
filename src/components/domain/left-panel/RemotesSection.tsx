import { useMemo, useState } from 'react'
import { ChevronRight, Folder, GitBranch, Plus } from 'lucide-react'
import { PendingIndicator } from '@/components/ui/pending-indicator'
import { cn } from '@/lib/utils'
import type { RemoteInfo } from '@/lib/bindings'
import { buildBranchTree, type BranchTreeNode } from '@/lib/branchTree'
import { detectProvider, RemoteIcon } from '@/lib/remoteProvider'
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuLabel,
  ContextMenuSeparator,
  ContextMenuTrigger,
} from '@/components/ui/context-menu'
import { Pencil, Target, Trash2 } from 'lucide-react'
import { useGitMutations } from '@/hooks/useGitMutations'
import { useUiStore } from '@/stores/uiStore'
import { useActiveRepo } from '@/stores/workspaceStore'
import { TooltipButton } from '@/components/ui/tooltip'

function BranchNode({ node, depth }: { node: BranchTreeNode; depth: number }) {
  const [open, setOpen] = useState(true)
  const pad = 24 + depth * 12

  if (node.branch === null) {
    return (
      <div>
        <button
          onClick={() => setOpen((o) => !o)}
          style={{ paddingLeft: pad }}
          className="flex w-full items-center gap-1.5 py-0.5 pr-3 text-left hover:bg-panel2"
        >
          <ChevronRight
            size={10}
            className={cn('flex-none text-muted-foreground transition-transform', open && 'rotate-90')}
          />
          <Folder size={11} className="flex-none text-muted-foreground" />
          <span className="truncate text-[11px] text-sub">{node.name}</span>
        </button>
        {open && node.children.map((c) => <BranchNode key={c.branch ?? c.name} node={c} depth={depth + 1} />)}
      </div>
    )
  }

  return (
    <div
      style={{ paddingLeft: pad + 14 }}
      className="flex items-center gap-1.5 py-0.5 pr-3 hover:bg-panel2"
    >
      <GitBranch size={10} className="flex-none text-muted-foreground" />
      <span className="truncate font-mono text-[11px] text-sub">{node.name}</span>
    </div>
  )
}

function RemoteNode({
  remote,
  onManage,
}: {
  remote: RemoteInfo
  onManage: () => void
}) {
  const [open, setOpen] = useState(true)
  const repo = useActiveRepo()
  const m = useGitMutations(repo?.id ?? null)
  const tree = useMemo(() => buildBranchTree(remote.branches), [remote.branches])
  const provider = detectProvider(remote.url)

  return (
    <div>
      <ContextMenu>
        <ContextMenuTrigger asChild>
          <button
            onClick={() => setOpen((o) => !o)}
            className="flex w-full items-center gap-1.5 py-1 pl-4 pr-3 text-left hover:bg-panel2"
          >
            <ChevronRight
              size={11}
              className={cn('flex-none text-muted-foreground transition-transform', open && 'rotate-90')}
            />
            <RemoteIcon provider={provider} width={12} height={12} className="flex-none text-sub" />
            <span className="truncate text-[11.5px] text-foreground">{remote.name}</span>
            <span className="ml-auto pl-1.5 font-mono text-[9px] text-muted-foreground">
              {remote.branches.length}
            </span>
          </button>
        </ContextMenuTrigger>
        <ContextMenuContent className="w-48">
          <ContextMenuLabel className="font-mono text-[11px] text-sub">{remote.name}</ContextMenuLabel>
          <ContextMenuSeparator />
          <ContextMenuItem onSelect={onManage}>
            <Pencil />
            Edit
          </ContextMenuItem>
          <ContextMenuItem
            disabled={remote.branches.length === 0 || m.setUpstream.isPending}
            onSelect={(e) => {
              e.preventDefault()
              m.setUpstream.mutate(`${remote.name}/${remote.branches[0]}`)
            }}
          >
            {m.setUpstream.isPending ? <PendingIndicator /> : <Target />}
            {m.setUpstream.isPending ? 'Setting target…' : 'Set target (upstream)'}
          </ContextMenuItem>
          <ContextMenuSeparator />
          <ContextMenuItem variant="destructive" onSelect={onManage}>
            <Trash2 />
            Delete
          </ContextMenuItem>
        </ContextMenuContent>
      </ContextMenu>
      {open && (
        <div className="pb-0.5">
          {remote.branches.length === 0 ? (
            <p className="py-0.5 pl-9 pr-3 text-[10px] text-muted-foreground">No branches</p>
          ) : (
            tree.map((n) => <BranchNode key={n.branch ?? n.name} node={n} depth={0} />)
          )}
        </div>
      )}
    </div>
  )
}

export function RemotesSection({
  remotes,
  onManage,
}: {
  remotes: RemoteInfo[]
  onManage: () => void
}) {
  const open = useUiStore((s) => s.sectionOpen.remote)
  const toggleSection = useUiStore((s) => s.toggleSection)

  return (
    <div className="group/section">
      <div
        onClick={() => toggleSection('remote')}
        className="flex cursor-pointer select-none items-center gap-1.5 py-1.5 pl-2.5 pr-3 hover:bg-panel2"
      >
        <ChevronRight
          size={12}
          strokeWidth={2.4}
          className={cn(
            'flex-none text-muted-foreground transition-transform duration-100',
            open && 'rotate-90'
          )}
        />
        <span className="text-[10px] font-bold tracking-[.09em] text-sub">REMOTES</span>
        <TooltipButton
          onClick={(e) => {
            e.stopPropagation()
            onManage()
          }}
          tooltip="Manage remotes"
          className="ml-auto flex size-4 flex-none items-center justify-center rounded text-muted-foreground opacity-0 hover:bg-panel3 hover:text-foreground focus:opacity-100 group-hover/section:opacity-100"
        >
          <Plus size={12} strokeWidth={2.4} />
        </TooltipButton>
        <span className="ml-1.5 font-mono text-[9.5px] text-muted-foreground">{remotes.length}</span>
      </div>
      {open && (
        <div className="pb-1">
          {remotes.length === 0 ? (
            <button
              onClick={onManage}
              className="flex w-full items-center gap-1.5 py-1 pl-6 pr-3 text-left text-[11px] text-muted-foreground hover:bg-panel2 hover:text-sub"
            >
              <Plus size={11} />
              Add a remote
            </button>
          ) : (
            remotes.map((r) => <RemoteNode key={r.name} remote={r} onManage={onManage} />)
          )}
        </div>
      )}
    </div>
  )
}
