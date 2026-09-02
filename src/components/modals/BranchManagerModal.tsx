import { useEffect, useMemo, useRef, useState } from 'react'
import { ArrowDown, ArrowUp, CloudOff, GitBranch, Monitor, Search, Sparkles } from 'lucide-react'
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
  deleteTargets,
  locationKey,
  locationsOf,
  matchesQuery,
  riskyLocations,
  rowCapabilities,
  sortRows,
  type BranchLocation,
  type BranchRow,
  type BranchSort,
  type SelectedLocation,
} from '@/lib/branchManager'
import { DisabledHint } from '@/components/ui/tooltip'
import { cn } from '@/lib/utils'

/**
 * The sync counts, as something you can spot without reading.
 *
 * These were plain grey text among other grey text, which is exactly the state
 * a person is scanning for when deciding what to clean up. Arrows carry the
 * direction and colour carries the urgency: green for work only you have,
 * amber for work you have not taken yet.
 */
function SyncBadges({ row }: { row: BranchRow }) {
  if (row.neverPushed) {
    return (
      <span className="inline-flex flex-none items-center gap-1 rounded-sm bg-[var(--gw-green)]/15 px-1.5 py-0.5 font-mono text-2xs text-[var(--gw-green)]">
        <Sparkles aria-hidden size={10} />
        never sent
      </span>
    )
  }
  if (row.upstreamGone) {
    return (
      <span className="inline-flex flex-none items-center gap-1 rounded-sm bg-modified/15 px-1.5 py-0.5 font-mono text-2xs text-modified">
        <CloudOff aria-hidden size={10} />
        server copy gone
      </span>
    )
  }
  if (!row.ahead && !row.behind) return null
  return (
    <span className="flex flex-none items-center gap-1">
      {row.ahead > 0 && (
        <span
          title={`${row.ahead} commit${row.ahead === 1 ? '' : 's'} here that the server does not have`}
          className="inline-flex items-center gap-0.5 rounded-sm bg-[var(--gw-green)]/15 px-1.5 py-0.5 font-mono text-2xs font-semibold text-[var(--gw-green)]"
        >
          <ArrowUp aria-hidden size={10} strokeWidth={2.6} />
          {row.ahead}
        </span>
      )}
      {row.behind > 0 && (
        <span
          title={`${row.behind} commit${row.behind === 1 ? '' : 's'} on the server you do not have yet`}
          className="inline-flex items-center gap-0.5 rounded-sm bg-modified/15 px-1.5 py-0.5 font-mono text-2xs font-semibold text-modified"
        >
          <ArrowDown aria-hidden size={10} strokeWidth={2.6} />
          {row.behind}
        </span>
      )}
    </span>
  )
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

  // Every ticked copy, resolved back to the row it belongs to. A copy that
  // vanished (deleted, or filtered out) must not stay counted.
  const selected: SelectedLocation[] = useMemo(() => {
    const out: SelectedLocation[] = []
    for (const row of visible) {
      for (const where of locationsOf(row)) {
        if (checked.has(locationKey(row.name, where))) out.push({ name: row.name, where, row })
      }
    }
    return out
  }, [visible, checked])
  const busy = m.deleteBranchesMany.isPending || m.pullBranchesMany.isPending

  /** Every copy that may be ticked, in display order, for range selection. */
  const allKeys = useMemo(
    () =>
      visible.flatMap((row) =>
        locationsOf(row)
          // The checked-out branch cannot be deleted, so its local copy is not
          // selectable -- a checkbox that does nothing is worse than none.
          .filter((where) => !(where === 'local' && row.isCurrent))
          .map((where) => locationKey(row.name, where)),
      ),
    [visible],
  )

  const toggle = (row: BranchRow, where: BranchLocation, shift: boolean) => {
    if (where === 'local' && row.isCurrent) return
    const key = locationKey(row.name, where)
    const next = new Set(checked)
    if (shift && anchor.current) {
      const from = allKeys.indexOf(anchor.current)
      const to = allKeys.indexOf(key)
      if (from !== -1 && to !== -1) {
        const [lo, hi] = from < to ? [from, to] : [to, from]
        for (const k of allKeys.slice(lo, hi + 1)) next.add(k)
        setChecked(next)
        return
      }
    }
    if (next.has(key)) next.delete(key)
    else next.add(key)
    anchor.current = key
    setChecked(next)
  }

  const allSelected = allKeys.length > 0 && allKeys.every((k) => checked.has(k))
  const toggleAll = () => {
    setChecked(allSelected ? new Set() : new Set(allKeys))
    anchor.current = null
  }

  // Only a local copy can be brought up to date, and only as a fast-forward.
  const pullable = useMemo(() => {
    const names = new Set<string>()
    for (const item of selected) {
      if (item.where === 'local' && rowCapabilities(item.row).canPull) names.add(item.name)
    }
    return [...names]
  }, [selected])
  const atRisk = riskyLocations(selected)
  // Counted separately so the confirmation can say what happens where, rather
  // than lumping a local tidy-up together with a deletion everyone else sees.
  const localCount = selected.filter((item) => item.where === 'local').length
  const serverCount = selected.length - localCount

  const doPull = () => {
    if (pullable.length === 0) return
    m.pullBranchesMany.mutate(pullable)
  }

  const doDelete = () => {
    m.deleteBranchesMany.mutate(deleteTargets(selected), {
      onSuccess: () => {
        setChecked(new Set())
        setConfirmDelete(false)
      },
    })
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
                disabled={allKeys.length === 0}
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
                const places = locationsOf(row)
                return (
                  <div
                    key={row.name}
                    className="rounded-md border border-border bg-panel px-3 py-2"
                  >
                    <div className="flex items-center gap-2">
                      <span className="min-w-0 flex-1 overflow-hidden text-ellipsis whitespace-nowrap font-mono text-xs">
                        {row.name}
                      </span>
                      {row.isCurrent && (
                        <span className="flex-none rounded-sm bg-accent-text/15 px-1.5 py-0.5 text-2xs text-accent-text">
                          current
                        </span>
                      )}
                      <SyncBadges row={row} />
                      {row.time != null && (
                        <span className="flex-none text-2xs text-muted-foreground">
                          {formatRelativeTime(row.time)}
                        </span>
                      )}
                    </div>
                    {/* One checkbox per copy. A branch on this computer and two
                        remotes is three things that can be deleted separately,
                        and a single row-level tick could only ever mean all. */}
                    <div className="mt-1.5 flex flex-wrap items-center gap-1.5">
                      {places.map((where) => {
                        const key = locationKey(row.name, where)
                        const isChecked = checked.has(key)
                        const locked = where === 'local' && row.isCurrent
                        const label = where === 'local' ? 'This computer' : where
                        return (
                          <label
                            key={key}
                            title={
                              locked
                                ? 'This is the branch you are on, so it cannot be deleted'
                                : `Select the copy on ${label}`
                            }
                            className={cn(
                              'inline-flex select-none items-center gap-1.5 rounded-[5px] border px-2 py-1 text-2xs',
                              locked
                                ? 'cursor-not-allowed border-border/60 text-muted-foreground opacity-60'
                                : 'cursor-pointer border-border hover:bg-panel3',
                              isChecked && 'border-accent-text/50 bg-accent-text/10 text-foreground',
                            )}
                          >
                            <input
                              type="checkbox"
                              checked={isChecked}
                              disabled={locked}
                              aria-label={`${row.name} on ${label}`}
                              onChange={() => {}}
                              onClick={(e) => {
                                toggle(row, where, e.shiftKey)
                              }}
                              className="size-3 flex-none accent-[var(--gw-accent)]"
                            />
                            {where === 'local' ? (
                              <Monitor aria-hidden size={10} className="flex-none" />
                            ) : (
                              <GitBranch aria-hidden size={10} className="flex-none" />
                            )}
                            {label}
                          </label>
                        )
                      })}
                    </div>
                  </div>
                )
              })}
            </div>
          </div>

          <div className="flex items-center gap-2 border-t border-border px-5 py-3">
            <span className="text-2xs text-sub">
              {selected.length > 0 ? `${plural(selected.length, 'copy', 'copies')} selected` : 'Nothing selected'}
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
        title={`Delete ${plural(selected.length, 'copy', 'copies')}?`}
        confirmLabel="Delete"
        pending={m.deleteBranchesMany.isPending}
        pendingLabel="Deleting…"
        keepOpenOnConfirm
        onConfirm={doDelete}
        description={
          <div className="space-y-2">
          {localCount > 0 && (
            <p>
              {plural(localCount, 'branch')} will be removed from this computer.
            </p>
          )}
          {serverCount > 0 && (
            <p>
              {plural(serverCount, 'copy', 'copies')} will be removed from the server, so
              {' '}
              <span className="text-removed">they disappear for everyone using them</span>.
            </p>
          )}
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
