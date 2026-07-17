import { useEffect, useMemo, useState } from 'react'
import { ChevronRight, Cloud, Folder, GitBranch, Pencil, Plus, Target, Trash2, X } from 'lucide-react'
import { detectProvider, RemoteIcon } from '@/lib/remoteProvider'
import { cn } from '@/lib/utils'
import { Button } from '@/components/ui/button'
import { PendingIndicator } from '@/components/ui/pending-indicator'
import { Input } from '@/components/ui/input'
import { Dialog, DialogContent, DialogHeader, DialogTitle } from '@/components/ui/dialog'
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuLabel,
  ContextMenuSeparator,
  ContextMenuTrigger,
} from '@/components/ui/context-menu'
import { ConfirmDialog } from '@/components/modals/ConfirmDialog'
import type { RemoteInfo } from '@/lib/bindings'
import { buildBranchTree, type BranchTreeNode } from '@/lib/branchTree'
import { useRemotes } from '@/hooks/useGitQueries'
import { useGitMutations } from '@/hooks/useGitMutations'
import { useUiStore } from '@/stores/uiStore'
import { useActiveRepo } from '@/stores/workspaceStore'

function BranchNode({
  node,
  depth,
  onSetUpstream,
  upstreamPending,
  upstreamTarget,
}: {
  node: BranchTreeNode
  depth: number
  onSetUpstream: (branch: string) => void
  upstreamPending: boolean
  upstreamTarget?: string
}) {
  const [open, setOpen] = useState(true)
  const isFolder = node.branch === null
  const pad = 8 + depth * 14

  if (isFolder) {
    return (
      <div>
        <button
          onClick={() => setOpen((o) => !o)}
          style={{ paddingLeft: pad }}
          className="flex w-full items-center gap-1.5 py-1 pr-2 text-left hover:bg-panel2"
        >
          <ChevronRight
            size={11}
            className={cn('flex-none text-muted-foreground transition-transform', open && 'rotate-90')}
          />
          <Folder size={12} className="flex-none text-muted-foreground" />
          <span className="truncate text-[11px] text-sub">{node.name}</span>
          <span className="ml-auto pl-1.5 font-mono text-[9px] text-muted-foreground">
            {node.children.length}
          </span>
        </button>
        {open &&
          node.children.map((c) => (
            <BranchNode
              key={c.branch ?? c.name}
              node={c}
              depth={depth + 1}
              onSetUpstream={onSetUpstream}
              upstreamPending={upstreamPending}
              upstreamTarget={upstreamTarget}
            />
          ))}
      </div>
    )
  }

  return (
    <div
      style={{ paddingLeft: pad + 16 }}
      className="group/branch flex items-center gap-1.5 py-1 pr-2 hover:bg-panel2"
    >
      <GitBranch size={11} className="flex-none text-muted-foreground" />
      <span className="truncate font-mono text-[11px] text-foreground">{node.name}</span>
      <button
        onClick={() => node.branch && onSetUpstream(node.branch)}
        title="Track this branch"
        disabled={upstreamPending}
        className={cn(
          'ml-auto flex-none rounded p-0.5 text-muted-foreground opacity-0 hover:text-primary disabled:pointer-events-none group-hover/branch:opacity-100',
          upstreamTarget === node.branch && 'text-primary opacity-100'
        )}
      >
        {upstreamTarget === node.branch ? <PendingIndicator className="size-3" /> : <Target size={12} />}
      </button>
    </div>
  )
}

// --- Remote row ----------------------------------------------------------

function RemoteRow({
  remote,
  onEdit,
  onDelete,
  onSetUpstream,
  upstreamPending,
  upstreamTarget,
}: {
  remote: RemoteInfo
  onEdit: () => void
  onDelete: () => void
  onSetUpstream: (branch: string) => void
  upstreamPending: boolean
  upstreamTarget?: string
}) {
  const [open, setOpen] = useState(true)
  const tree = useMemo(() => buildBranchTree(remote.branches), [remote.branches])
  const provider = detectProvider(remote.url)

  return (
    <div className="rounded-md border border-border bg-background">
      <ContextMenu>
        <ContextMenuTrigger asChild>
          <div className="flex items-center gap-2 px-2.5 py-2">
            <button
              onClick={() => setOpen((o) => !o)}
              className="flex min-w-0 flex-1 items-center gap-2 text-left"
            >
              <ChevronRight
                size={12}
                className={cn(
                  'flex-none text-muted-foreground transition-transform',
                  open && 'rotate-90'
                )}
              />
              <RemoteIcon provider={provider} width={13} height={13} className="flex-none text-sub" />
              <span className="flex-none text-[12px] font-semibold text-foreground">{remote.name}</span>
              <span className="truncate font-mono text-[10.5px] text-muted-foreground">
                {remote.url}
              </span>
              <span className="ml-auto flex-none pl-2 font-mono text-[9px] text-muted-foreground">
                {remote.branches.length}
              </span>
            </button>
            <button
              onClick={onEdit}
              title="Edit remote"
              disabled={upstreamPending}
              className="flex-none rounded p-1 text-muted-foreground hover:text-foreground disabled:pointer-events-none disabled:opacity-40"
            >
              <Pencil size={12} />
            </button>
            <button
              onClick={onDelete}
              title="Delete remote"
              disabled={upstreamPending}
              className="flex-none rounded p-1 text-muted-foreground hover:text-removed disabled:pointer-events-none disabled:opacity-40"
            >
              <Trash2 size={12} />
            </button>
          </div>
        </ContextMenuTrigger>
        <ContextMenuContent className="w-48">
          <ContextMenuLabel className="font-mono text-[11px] text-sub">{remote.name}</ContextMenuLabel>
          <ContextMenuSeparator />
          <ContextMenuItem disabled={upstreamPending} onSelect={onEdit}>
            <Pencil />
            Edit
          </ContextMenuItem>
          <ContextMenuItem
            disabled={remote.branches.length === 0 || upstreamPending}
            onSelect={(e) => {
              e.preventDefault()
              onSetUpstream(`${remote.name}/${remote.branches[0]}`)
            }}
          >
            {upstreamTarget === `${remote.name}/${remote.branches[0]}` ? <PendingIndicator /> : <Target />}
            {upstreamTarget === `${remote.name}/${remote.branches[0]}` ? 'Setting target…' : 'Set target (upstream)'}
          </ContextMenuItem>
          <ContextMenuSeparator />
          <ContextMenuItem disabled={upstreamPending} variant="destructive" onSelect={onDelete}>
            <Trash2 />
            Delete
          </ContextMenuItem>
        </ContextMenuContent>
      </ContextMenu>

      {open && (
        <div className="border-t border-border py-1">
          {remote.branches.length === 0 ? (
            <p className="px-4 py-1.5 text-[10.5px] text-muted-foreground">
              No branches yet. Fetch to see this remote's branches.
            </p>
          ) : (
            tree.map((n) => (
              <BranchNode
                key={n.branch ?? n.name}
                node={n}
                depth={0}
                onSetUpstream={onSetUpstream}
                upstreamPending={upstreamPending}
                upstreamTarget={upstreamTarget}
              />
            ))
          )}
        </div>
      )}
    </div>
  )
}

// --- Modal ---------------------------------------------------------------

type Editing = { name: string; url: string; original: string } | null

export function RemotesModal() {
  const open = useUiStore((s) => s.activeModal === 'remotes')
  const closeModal = useUiStore((s) => s.closeModal)

  const repo = useActiveRepo()
  const remotes = useRemotes(repo?.id ?? null)
  const m = useGitMutations(repo?.id ?? null)

  const [adding, setAdding] = useState(false)
  const [newName, setNewName] = useState('')
  const [newUrl, setNewUrl] = useState('')
  const [editing, setEditing] = useState<Editing>(null)
  const [deleteTarget, setDeleteTarget] = useState<string | null>(null)

  useEffect(() => {
    if (!open) {
      setAdding(false)
      setNewName('')
      setNewUrl('')
      setEditing(null)
      setDeleteTarget(null)
    }
  }, [open])

  const existingNames = useMemo(
    () => new Set((remotes.data ?? []).map((r) => r.name)),
    [remotes.data]
  )

  const canAdd =
    newName.trim() !== '' &&
    newUrl.trim() !== '' &&
    !existingNames.has(newName.trim()) &&
    !m.addRemote.isPending
  const editPending = m.renameRemote.isPending || m.setRemoteUrl.isPending
  const upstreamTarget = m.setUpstream.isPending ? m.setUpstream.variables : undefined

  const submitAdd = () => {
    if (!canAdd) return
    m.addRemote.mutate(
      { name: newName.trim(), url: newUrl.trim() },
      {
        onSuccess: () => {
          setAdding(false)
          setNewName('')
          setNewUrl('')
        },
      }
    )
  }

  const submitEdit = () => {
    if (!editing) return
    const name = editing.name.trim()
    const url = editing.url.trim()
    if (name === '' || url === '') return
    // Rename first if the name changed, then update the URL.
    const afterRename = () => {
      m.setRemoteUrl.mutate({ name, url }, { onSuccess: () => setEditing(null) })
    }
    if (name !== editing.original) {
      m.renameRemote.mutate({ name: editing.original, newName: name }, { onSuccess: afterRename })
    } else {
      afterRename()
    }
  }

  return (
    <>
      <Dialog open={open} onOpenChange={(o) => !o && closeModal()}>
        <DialogContent className="gap-0 p-0 sm:max-w-lg" aria-describedby={undefined}>
          <DialogHeader className="border-b border-border px-4 pb-3 pt-4">
            <DialogTitle className="flex items-center gap-2 text-sm">
              <Cloud size={15} strokeWidth={1.9} />
              Manage remotes
            </DialogTitle>
          </DialogHeader>

          <div className="grid max-h-[60vh] gap-2 overflow-y-auto px-4 py-4">
            {(remotes.data?.length ?? 0) === 0 && !adding && (
              <p className="py-2 text-center text-[11.5px] text-muted-foreground">
                No remotes yet. Add one to push and pull your work.
              </p>
            )}

            {(remotes.data ?? []).map((r) =>
              editing?.original === r.name ? (
                <div key={r.name} className="grid gap-2 rounded-md border border-primary bg-panel2 p-3">
                  <div className="grid gap-1.5">
                    <label className="text-[10.5px] font-semibold text-sub">Name</label>
                    <Input
                      value={editing.name}
                      onChange={(e) => setEditing({ ...editing, name: e.target.value })}
                      className="h-auto bg-background py-1.5 font-mono text-xs"
                      autoFocus
                      disabled={editPending}
                    />
                  </div>
                  <div className="grid gap-1.5">
                    <label className="text-[10.5px] font-semibold text-sub">URL</label>
                    <Input
                      value={editing.url}
                      onChange={(e) => setEditing({ ...editing, url: e.target.value })}
                      onKeyDown={(e) => e.key === 'Enter' && submitEdit()}
                      className="h-auto bg-background py-1.5 font-mono text-xs"
                      disabled={editPending}
                    />
                  </div>
                  <div className="flex justify-end gap-2 pt-1">
                    <Button variant="secondary" size="sm" disabled={editPending} onClick={() => setEditing(null)}>
                      Cancel
                    </Button>
                    <Button size="sm" disabled={editPending} aria-busy={editPending || undefined} onClick={submitEdit}>
                      {editPending && <PendingIndicator />}
                      {editPending ? 'Saving…' : 'Save'}
                    </Button>
                  </div>
                </div>
              ) : (
                <RemoteRow
                  key={r.name}
                  remote={r}
                  onEdit={() => setEditing({ name: r.name, url: r.url, original: r.name })}
                  onDelete={() => setDeleteTarget(r.name)}
                  onSetUpstream={(branch) => m.setUpstream.mutate(branch)}
                  upstreamPending={m.setUpstream.isPending}
                  upstreamTarget={upstreamTarget}
                />
              )
            )}

            {adding ? (
              <div className="grid gap-2 rounded-md border border-primary bg-panel2 p-3">
                <div className="grid gap-1.5">
                  <label className="text-[10.5px] font-semibold text-sub">Name</label>
                  <Input
                    value={newName}
                    onChange={(e) => setNewName(e.target.value)}
                    placeholder="origin"
                    className="h-auto bg-background py-1.5 font-mono text-xs"
                    autoFocus
                    disabled={m.addRemote.isPending}
                  />
                  {existingNames.has(newName.trim()) && newName.trim() !== '' && (
                    <p className="text-[10px] text-removed">That name is already used.</p>
                  )}
                </div>
                <div className="grid gap-1.5">
                  <label className="text-[10.5px] font-semibold text-sub">URL</label>
                  <Input
                    value={newUrl}
                    onChange={(e) => setNewUrl(e.target.value)}
                    onKeyDown={(e) => e.key === 'Enter' && submitAdd()}
                    placeholder="https://github.com/you/repo.git"
                    className="h-auto bg-background py-1.5 font-mono text-xs"
                    disabled={m.addRemote.isPending}
                  />
                </div>
                <div className="flex justify-end gap-2 pt-1">
                  <Button variant="secondary" size="sm" disabled={m.addRemote.isPending} onClick={() => setAdding(false)}>
                    Cancel
                  </Button>
                  <Button size="sm" disabled={!canAdd} onClick={submitAdd}>
                    {m.addRemote.isPending && <PendingIndicator />}
                    {m.addRemote.isPending ? 'Adding…' : 'Add remote'}
                  </Button>
                </div>
              </div>
            ) : (
              <button
                onClick={() => setAdding(true)}
                className="flex items-center justify-center gap-1.5 rounded-md border border-dashed border-border py-2 text-[11.5px] text-sub hover:border-primary hover:text-primary"
              >
                <Plus size={13} />
                Add remote
              </button>
            )}
          </div>

          <div className="flex justify-end border-t border-border px-4 py-3">
            <Button variant="secondary" size="sm" onClick={closeModal}>
              <X size={13} />
              Close
            </Button>
          </div>
        </DialogContent>
      </Dialog>

      <ConfirmDialog
        open={deleteTarget !== null}
        onOpenChange={(o) => !o && setDeleteTarget(null)}
        title="Delete remote?"
        destructive
        description={
          <>
            This removes <span className="font-mono text-foreground">{deleteTarget}</span> and its
            remote branches from your local repo. Your commits stay; nothing is deleted on the
            server.
          </>
        }
        confirmLabel="Delete remote"
        pending={m.removeRemote.isPending}
        pendingLabel="Deleting remote…"
        keepOpenOnConfirm
        onConfirm={() => {
          if (deleteTarget) {
            m.removeRemote.mutate(deleteTarget, { onSuccess: () => setDeleteTarget(null) })
          }
        }}
      />
    </>
  )
}
