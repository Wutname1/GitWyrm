import { useEffect, useMemo, useRef, useState } from 'react'
import { GitBranch, Search } from 'lucide-react'
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from '@/components/ui/dialog'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { ConfirmDialog } from '@/components/modals/ConfirmDialog'
import { useUiStore } from '@/stores/uiStore'
import { useActiveRepo } from '@/stores/workspaceStore'
import { useBranches, useRemotes } from '@/hooks/useGitQueries'
import { useGitMutations } from '@/hooks/useGitMutations'
import { formatRelativeTime, plural } from '@/lib/gitDisplay'
import {
  buildBranchRows,
  matchesQuery,
  riskyRows,
  rowCapabilities,
  sortRows,
  type BranchRow,
  type BranchSort,
} from '@/lib/branchManager'
import { DisabledHint } from '@/components/ui/tooltip'
import { cn } from '@/lib/utils'

/** Where a branch's copies live, in words rather than git's vocabulary. */
function whereItLives(row: BranchRow): string {
  if (row.local && row.remote) return `On this computer and on ${row.remote}`
  if (row.local) return 'Only on this computer'
  return `Only on ${row.remote ?? 'the server'}`
}

/** The one-line state badge, or null when there is nothing worth saying. */
function statusOf(row: BranchRow): { text: string; tone: string } | null {
  if (row.isCurrent) return { text: 'current', tone: 'text-accent-text' }
  if (row.neverPushed) return { text: 'never sent', tone: 'text-modified' }
  if (row.upstreamGone) return { text: 'server copy gone', tone: 'text-modified' }
  if (row.ahead && row.behind) return { text: `${row.ahead} to send, ${row.behind} to get`, tone: 'text-modified' }
  if (row.ahead) return { text: `${plural(row.ahead, 'commit')} to send`, tone: 'text-modified' }
  if (row.behind) return { text: `${plural(row.behind, 'commit')} to get`, tone: 'text-sub' }
  return null
}

export function BranchManagerModal() {
  const open = useUiStore((s) => s.activeModal === 'branchManager')
  const closeModal = useUiStore((s) => s.closeModal)
  const repo = useActiveRepo()
  const branches = useBranches(repo?.id ?? null)
  const remotes = useRemotes(repo?.id ?? null)
  const m = useGitMutations(repo?.id ?? null)

  const [query, setQuery] = useState('')
  const [sort, setSort] = useState<BranchSort>('name')
  const [checked, setChecked] = useState<Set<string>>(new Set())
  const [confirmDelete, setConfirmDelete] = useState(false)
  // Anchor for shift-range selection, mirroring the graph's commit selection.
  const anchor = useRef<string | null>(null)

  // Everything resets on close, so reopening never starts mid-decision with a
  // stale selection of branches that may no longer exist.
  useEffect(() => {
    if (!open) {
      setQuery('')
      setChecked(new Set())
      setConfirmDelete(false)
      anchor.current = null
    }
  }, [open])

  const rows = useMemo(
    () => buildBranchRows(branches.data?.local ?? [], remotes.data ?? []),
    [branches.data, remotes.data],
  )
  const visible = useMemo(
    () => sortRows(rows.filter((r) => matchesQuery(r, query)), sort),
    [rows, query, sort],
  )

  // A branch that disappeared (deleted, or filtered out) must not stay counted.
  const selected = useMemo(
    () => visible.filter((r) => checked.has(r.name) && !r.isCurrent),
    [visible, checked],
  )
  const busy = m.deleteBranchesMany.isPending || m.pullBranchesMany.isPending

  const toggle = (row: BranchRow, mods: { shift: boolean; ctrl: boolean }) => {
    // The current branch cannot be deleted or pulled onto, so it is not
    // selectable -- offering a checkbox that does nothing is worse than none.
    if (row.isCurrent) return
    const next = new Set(checked)
    if (mods.shift && anchor.current) {
      const names = visible.map((r) => r.name)
      const from = names.indexOf(anchor.current)
      const to = names.indexOf(row.name)
      if (from !== -1 && to !== -1) {
        const [lo, hi] = from < to ? [from, to] : [to, from]
        for (const r of visible.slice(lo, hi + 1)) {
          if (!r.isCurrent) next.add(r.name)
        }
        setChecked(next)
        return
      }
    }
    if (next.has(row.name)) next.delete(row.name)
    else next.add(row.name)
    anchor.current = row.name
    setChecked(next)
  }

  const selectable = visible.filter((r) => !r.isCurrent)
  const allSelected = selectable.length > 0 && selectable.every((r) => checked.has(r.name))
  const toggleAll = () => {
    setChecked(allSelected ? new Set() : new Set(selectable.map((r) => r.name)))
    anchor.current = null
  }

  const pullable = selected.filter((r) => rowCapabilities(r).canPull)
  const atRisk = riskyRows(selected)

  const doPull = () => {
    if (pullable.length === 0) return
    m.pullBranchesMany.mutate(pullable.map((r) => r.name))
  }

  const doDelete = () => {
    m.deleteBranchesMany.mutate(
      selected.map((r) => ({ name: r.name, local: !!r.local, remote: r.remote })),
      {
        onSuccess: () => {
          setChecked(new Set())
          setConfirmDelete(false)
        },
      },
    )
  }

  return (
    <>
      <Dialog open={open} onOpenChange={(next) => !next && !busy && closeModal()}>
        <DialogContent className="flex max-h-[min(760px,calc(100vh-64px))] flex-col gap-0 overflow-hidden p-0 sm:max-w-3xl">
          <DialogHeader className="border-b border-border px-5 py-4 pr-12">
            <DialogTitle className="flex items-center gap-2 text-sm">
              <GitBranch aria-hidden size={14} />
              Branches
            </DialogTitle>
            <DialogDescription className="text-2xs">
              Every branch, wherever it lives. Tick the ones you want to update or delete.
            </DialogDescription>
          </DialogHeader>

          <div className="flex items-center gap-2 border-b border-border px-5 py-2.5">
            <label className="relative w-64">
              <Search aria-hidden size={13} className="absolute left-2.5 top-1/2 -translate-y-1/2 text-muted-foreground" />
              <Input
                value={query}
                onChange={(e) => setQuery(e.target.value)}
                placeholder="Find a branch"
                aria-label="Find a branch"
                className="h-8 bg-background pl-8 text-xs"
              />
            </label>
            <Button
              variant="ghost"
              size="sm"
              className="h-8 text-2xs"
              onClick={() => setSort(sort === 'name' ? 'stale' : 'name')}
            >
              {sort === 'name' ? 'Sort by name' : 'Oldest first'}
            </Button>
            <div className="ml-auto flex items-center gap-2">
              <Button
                variant="ghost"
                size="sm"
                className="h-8 text-2xs"
                onClick={toggleAll}
                disabled={selectable.length === 0}
              >
                {allSelected ? 'Clear all' : 'Select all'}
              </Button>
            </div>
          </div>

          <div className="min-h-0 flex-1 overflow-y-auto px-3 py-2">
            {visible.length === 0 && (
              <div className="grid min-h-48 place-items-center text-xs text-muted-foreground">
                {query ? 'No branches match that.' : 'No branches yet.'}
              </div>
            )}
            <div className="space-y-1">
              {visible.map((row) => {
                const status = statusOf(row)
                const isChecked = checked.has(row.name)
                return (
                  <div
                    key={row.name}
                    onClick={(e) => toggle(row, { shift: e.shiftKey, ctrl: e.ctrlKey || e.metaKey })}
                    className={cn(
                      'flex items-center gap-3 rounded-md border border-border bg-panel px-3 py-2',
                      row.isCurrent ? 'opacity-70' : 'cursor-pointer hover:bg-panel3',
                      isChecked && 'border-accent-text/40 bg-panel3',
                    )}
                  >
                    <input
                      type="checkbox"
                      checked={isChecked}
                      disabled={row.isCurrent}
                      aria-label={`Select ${row.name}`}
                      onChange={() => {}}
                      onClick={(e) => e.stopPropagation()}
                      onPointerDown={(e) => {
                        e.stopPropagation()
                        toggle(row, { shift: e.shiftKey, ctrl: e.ctrlKey || e.metaKey })
                      }}
                      className="size-3.5 flex-none accent-[var(--gw-accent)]"
                    />
                    <div className="min-w-0 flex-1">
                      <div className="flex items-center gap-2">
                        <span className="overflow-hidden text-ellipsis whitespace-nowrap font-mono text-xs">
                          {row.name}
                        </span>
                        {status && <span className={cn('flex-none text-2xs', status.tone)}>{status.text}</span>}
                      </div>
                      <div className="mt-0.5 overflow-hidden text-ellipsis whitespace-nowrap text-2xs text-sub">
                        {whereItLives(row)}
                        {row.time != null && ` · ${formatRelativeTime(row.time)}`}
                      </div>
                    </div>
                  </div>
                )
              })}
            </div>
          </div>

          <div className="flex items-center gap-2 border-t border-border px-5 py-3">
            <span className="text-2xs text-sub">
              {selected.length > 0 ? `${plural(selected.length, 'branch')} selected` : 'Nothing selected'}
            </span>
            <div className="ml-auto flex items-center gap-2">
              <Button variant="ghost" size="sm" onClick={() => closeModal()} disabled={busy}>
                Close
              </Button>
              <DisabledHint
                disabled={busy || pullable.length === 0}
                reason={
                  selected.length > 0 && pullable.length === 0
                    ? 'None of these can be brought up to date on their own - they have commits of their own to combine first'
                    : 'Tick a branch that has new commits to get'
                }
              >
                <Button
                  variant="secondary"
                  size="sm"
                  onClick={doPull}
                  disabled={busy || pullable.length === 0}
                >
                  {m.pullBranchesMany.isPending ? 'Getting…' : `Get latest${pullable.length ? ` (${pullable.length})` : ''}`}
                </Button>
              </DisabledHint>
              <Button
                variant="destructive"
                size="sm"
                onClick={() => setConfirmDelete(true)}
                disabled={busy || selected.length === 0}
              >
                Delete{selected.length ? ` (${selected.length})` : ''}
              </Button>
            </div>
          </div>
        </DialogContent>
      </Dialog>

      <ConfirmDialog
        open={confirmDelete}
        onOpenChange={setConfirmDelete}
        destructive
        title={`Delete ${plural(selected.length, 'branch')}?`}
        confirmLabel="Delete"
        pending={m.deleteBranchesMany.isPending}
        pendingLabel="Deleting…"
        keepOpenOnConfirm
        onConfirm={doDelete}
        description={
          <div className="space-y-2">
          <p>
            {selected.some((r) => r.remote) ? (
              <>
                Some of these will be removed from the server, so they disappear{' '}
                <span className="text-removed">for everyone using them</span>.
              </>
            ) : (
              <>These will be removed from this computer.</>
            )}
          </p>
          {atRisk.length > 0 && (
            // The single-branch confirm only hedged with "may become hard to
            // find". Deleting ten at once is a much larger blast radius, so the
            // ones that actually lose work are named here.
            <div className="rounded-md border border-modified/40 bg-modified/10 px-2.5 py-2">
              <div className="text-2xs font-semibold text-modified">
                {plural(atRisk.length, 'branch')} with work that is not saved anywhere else:
              </div>
              <div className="mt-1 font-mono text-2xs">
                {atRisk.slice(0, 6).map((r) => r.name).join(', ')}
                {atRisk.length > 6 && ` and ${atRisk.length - 6} more`}
              </div>
            </div>
          )}
          </div>
        }
      />
    </>
  )
}
