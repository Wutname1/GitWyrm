import { useEffect, useMemo, useRef, useState, useSyncExternalStore, type ReactElement, type ReactNode } from 'react'
import { ArrowDown, ArrowUp, ChevronDown, Cloud, CloudOff, Eye, EyeOff, GitBranch, Lock, Monitor, Search, Sparkles } from 'lucide-react'
import { toast } from 'sonner'
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from '@/components/ui/dialog'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu'
import { ConfirmDialog } from '@/components/modals/ConfirmDialog'
import { useUiStore } from '@/stores/uiStore'
import { useActiveRepo } from '@/stores/workspaceStore'
import { useBranches, useRemotes } from '@/hooks/useGitQueries'
import { getBulkProgress, subscribeBulkProgress, useGitMutations } from '@/hooks/useGitMutations'
import { formatRelativeTime, plural } from '@/lib/gitDisplay'
import {
  buildBranchRows,
  deleteTargets,
  locationKey,
  locationsOf,
  matchesQuery,
  remoteBranchForRow,
  riskyLocations,
  selectedBranchActions,
  sortRows,
  type BranchLocation,
  type BranchRow,
  type BranchSort,
  type SelectedLocation,
} from '@/lib/branchManager'
import { DisabledHint, TooltipHint } from '@/components/ui/tooltip'
import { cn } from '@/lib/utils'
import {
  branchVisibilityFor,
  isBranchVisible,
  useBranchVisibilityStore,
} from '@/stores/branchVisibilityStore'
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuGroup,
  ContextMenuLabel,
  ContextMenuSeparator,
  ContextMenuTrigger,
} from '@/components/ui/context-menu'
import { BranchMenu } from '@/components/domain/branch/BranchMenu'
import { RemoteBranchMenuItems } from '@/components/domain/branch/RemoteBranchMenuItems'
import type { RemoteBranchInfo, RemoteInfo } from '@/lib/bindings'

/**
 * Explain sync state in the words a person needs for the next action. Arrows
 * still make the direction easy to spot, but the meaning never depends on
 * knowing what an unlabeled git arrow means.
 */
function BranchStatus({ row }: { row: BranchRow }) {
  if (row.neverPushed) {
    return (
      <span className="inline-flex items-center gap-1 text-2xs font-medium text-modified">
        <Sparkles aria-hidden size={10} />
        Only on this computer
      </span>
    )
  }
  if (row.upstreamGone) {
    return (
      <span className="inline-flex items-center gap-1 text-2xs font-medium text-modified">
        <CloudOff aria-hidden size={10} />
        Shared copy gone
      </span>
    )
  }
  if (!row.local) {
    return <span className="text-2xs text-muted-foreground">Shared only</span>
  }
  if (!row.ahead && !row.behind) {
    return (
      <span className="text-2xs text-muted-foreground">
        {row.remotes.length > 0 ? 'Up to date' : 'This computer only'}
      </span>
    )
  }
  return (
    <span className="flex flex-wrap items-center gap-x-2 gap-y-1">
      {row.ahead > 0 && (
        <span
          title={`${row.ahead} commit${row.ahead === 1 ? '' : 's'} here that the shared copy does not have`}
          className="inline-flex items-center gap-1 text-2xs font-medium text-[var(--gw-green)]"
        >
          <ArrowUp aria-hidden size={10} strokeWidth={2.6} />
          {row.ahead} to send
        </span>
      )}
      {row.behind > 0 && (
        <span
          title={`${row.behind} commit${row.behind === 1 ? '' : 's'} in the shared copy that you do not have yet`}
          className="inline-flex items-center gap-1 text-2xs font-medium text-modified"
        >
          <ArrowDown aria-hidden size={10} strokeWidth={2.6} />
          {row.behind} to get
        </span>
      )}
    </span>
  )
}

/** Find the exact remote branch represented by one location in a joined row. */
function remoteBranchName(remote: RemoteInfo, branch: RemoteBranchInfo): string {
  const prefix = `${remote.name}/`
  return branch.name.startsWith(prefix) ? branch.name.slice(prefix.length) : branch.name
}

function RemoteLocationMenu({
  remote,
  row,
  repoId,
  busy,
}: {
  remote: RemoteInfo
  row: BranchRow
  repoId: string | null
  busy: boolean
}) {
  const branch = remoteBranchForRow(remote, row)
  if (!branch) return null
  return (
    <RemoteBranchMenuItems
      remote={remote}
      branch={remoteBranchName(remote, branch)}
      repoId={repoId}
      localCounterpart={branch.local_counterpart}
      trackedBy={branch.tracked_by}
      tip={branch.tip}
      opInProgress={busy}
    />
  )
}

/** A right-click menu scoped to exactly one local or shared copy. */
function LocationMenu({
  row,
  where,
  remotes,
  repoId,
  busy,
  tooltip,
  children,
}: {
  row: BranchRow
  where: BranchLocation
  remotes: RemoteInfo[]
  repoId: string | null
  busy: boolean
  tooltip?: ReactNode
  children: ReactElement
}) {
  const remote = where === 'local' ? null : remotes.find((item) => item.name === where) ?? null
  const remoteBranch = remote ? remoteBranchForRow(remote, row) : null
  const trigger = <ContextMenuTrigger asChild>{children}</ContextMenuTrigger>
  return (
    <ContextMenu>
      {tooltip ? <TooltipHint label={tooltip}>{trigger}</TooltipHint> : trigger}
      <ContextMenuContent className="w-72">
        <ContextMenuLabel className="font-mono text-2xs text-sub">
          {where === 'local' ? row.name : `${where}/${remoteBranch?.name ?? row.name}`}
        </ContextMenuLabel>
        <ContextMenuSeparator />
        {where === 'local' && row.local ? (
          <BranchMenu branch={row.local} />
        ) : remote ? (
          <RemoteLocationMenu remote={remote} row={row} repoId={repoId} busy={busy} />
        ) : null}
      </ContextMenuContent>
    </ContextMenu>
  )
}

/** The row menu deliberately includes every place this branch exists. */
function RowMenu({
  row,
  remotes,
  repoId,
  busy,
  children,
}: {
  row: BranchRow
  remotes: RemoteInfo[]
  repoId: string | null
  busy: boolean
  children: ReactElement
}) {
  const rowRemotes = row.remotes
    .map((name) => remotes.find((remote) => remote.name === name) ?? null)
    .filter((remote): remote is RemoteInfo => remote != null)
  return (
    <ContextMenu>
      <ContextMenuTrigger asChild>{children}</ContextMenuTrigger>
      <ContextMenuContent className="w-72">
        <ContextMenuLabel className="font-mono text-2xs text-sub">{row.name}</ContextMenuLabel>
        {row.local && (
          <>
            <ContextMenuSeparator />
            <ContextMenuLabel className="text-2xs text-muted-foreground">This computer</ContextMenuLabel>
            <BranchMenu branch={row.local} showWebLink={rowRemotes.length === 0} />
          </>
        )}
        {rowRemotes.map((remote) => (
          <ContextMenuGroup key={remote.name}>
            <ContextMenuSeparator />
            <ContextMenuLabel className="text-2xs text-muted-foreground">
              Shared ({remote.name})
            </ContextMenuLabel>
            <RemoteLocationMenu remote={remote} row={row} repoId={repoId} busy={busy} />
          </ContextMenuGroup>
        ))}
      </ContextMenuContent>
    </ContextMenu>
  )
}

export function BranchManagerModal() {
  const open = useUiStore((s) => s.activeModal === 'branchManager')
  const closeModal = useUiStore((s) => s.closeModal)
  const repo = useActiveRepo()
  const branches = useBranches(repo?.id ?? null)
  const remotes = useRemotes(repo?.id ?? null)
  const m = useGitMutations(repo?.id ?? null)
  const visibility = useBranchVisibilityStore((s) => branchVisibilityFor(s.byRepo, repo?.id ?? null))
  const hideBranch = useBranchVisibilityStore((s) => s.hideBranch)
  const showBranch = useBranchVisibilityStore((s) => s.showBranch)
  const showAllBranches = useBranchVisibilityStore((s) => s.showAllBranches)

  const [query, setQuery] = useState('')
  const [sort, setSort] = useState<BranchSort>('name')
  const [checked, setChecked] = useState<Set<string>>(new Set())
  const [confirmDelete, setConfirmDelete] = useState(false)
  // Progress for whichever bulk run is going, read from the mutations module
  // so it updates per item rather than only when the whole run settles.
  const progress = useSyncExternalStore(subscribeBulkProgress, getBulkProgress, getBulkProgress)
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
  const remoteList = remotes.data ?? []
  const visible = useMemo(
    () => sortRows(rows.filter((r) => matchesQuery(r, query)), sort),
    [rows, query, sort],
  )
  const allBranchNames = useMemo(() => rows.map((row) => row.name), [rows])
  const hasGraphFilter = visibility.focused != null || visibility.hidden.length > 0

  const toggleGraphVisibility = (name: string) => {
    if (!repo) return
    if (isBranchVisible(visibility, name)) {
      hideBranch(repo.id, name)
      toast(`${name} hidden from the graph`)
    } else {
      showBranch(repo.id, name, allBranchNames)
      toast(`${name} shown in the graph`)
    }
  }

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
  const busy =
    m.deleteBranchesMany.isPending ||
    m.pullBranchesMany.isPending ||
    m.pushBranchesMany.isPending ||
    m.copyRemoteBranchesMany.isPending

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

  const availableActions = useMemo(
    () => selectedBranchActions(selected, remoteList),
    [selected, remoteList],
  )
  const { pullable, sendable } = availableActions
  const atRisk = riskyLocations(selected)
  // Counted separately so the confirmation can say what happens where, rather
  // than lumping a local tidy-up together with a deletion everyone else sees.
  const localCount = selected.filter((item) => item.where === 'local').length
  const serverCount = selected.length - localCount
  const selectedBranchCount = new Set(selected.map((item) => item.name)).size

  const doPull = () => {
    if (pullable.length === 0) return
    m.pullBranchesMany.mutate(pullable)
  }

  const doSend = () => {
    if (sendable.length === 0) return
    m.pushBranchesMany.mutate(sendable)
  }

  const doCopyHere = () => {
    if (availableActions.copyTargets.length === 0 || availableActions.copyAmbiguous) return
    m.copyRemoteBranchesMany.mutate(availableActions.copyTargets)
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
        <DialogContent className="flex max-h-[min(760px,calc(100vh-64px))] flex-col gap-0 overflow-hidden p-0 sm:max-w-4xl">
          <DialogHeader className="border-b border-border px-5 py-4 pr-12">
            <DialogTitle className="flex items-center gap-2 text-sm">
              <GitBranch aria-hidden size={14} />
              Branches
            </DialogTitle>
            <DialogDescription className="text-2xs">
              Choose what appears in the graph, or update and remove branch copies.
            </DialogDescription>
          </DialogHeader>

          <div className="flex flex-wrap items-center gap-2 border-b border-border px-5 py-2.5">
            <label className="relative min-w-48 flex-1">
              <Search aria-hidden size={13} className="absolute left-2.5 top-1/2 -translate-y-1/2 text-muted-foreground" />
              <Input
                value={query}
                onChange={(e) => setQuery(e.target.value)}
                placeholder="Find a branch"
                aria-label="Find a branch"
                className="h-8 bg-background pl-8 text-xs"
              />
            </label>
            <DropdownMenu>
              <DropdownMenuTrigger asChild>
                <Button variant="outline" size="sm" className="h-8 min-w-28 justify-between text-2xs">
                  {sort === 'name' ? 'Name A–Z' : 'Oldest first'}
                  <ChevronDown aria-hidden size={12} />
                </Button>
              </DropdownMenuTrigger>
              <DropdownMenuContent align="end" className="min-w-36">
                <DropdownMenuRadioGroup
                  value={sort}
                  onValueChange={(value) => {
                    if (value === 'name' || value === 'stale') setSort(value)
                  }}
                >
                  <DropdownMenuRadioItem value="name" className="text-xs">Name A–Z</DropdownMenuRadioItem>
                  <DropdownMenuRadioItem value="stale" className="text-xs">Oldest first</DropdownMenuRadioItem>
                </DropdownMenuRadioGroup>
              </DropdownMenuContent>
            </DropdownMenu>
            <Button
              variant="ghost"
              size="sm"
              className="h-8 text-2xs"
              onClick={toggleAll}
              disabled={allKeys.length === 0}
            >
              {allSelected ? 'Clear selection' : 'Select shown'}
            </Button>
            {hasGraphFilter && repo && (
              <Button
                variant="ghost"
                size="sm"
                className="h-8 text-2xs"
                onClick={() => {
                  showAllBranches(repo.id)
                  toast('All branches shown')
                }}
              >
                <Eye aria-hidden size={13} />
                Show all
              </Button>
            )}
          </div>

          <div className="min-h-0 flex-1 overflow-y-auto px-4 py-3">
            {visible.length === 0 && (
              <div className="grid min-h-48 place-items-center text-xs text-muted-foreground">
                {query ? 'No branches match that.' : 'No branches yet.'}
              </div>
            )}
            {visible.length > 0 && (
              <div className="grid grid-cols-[minmax(0,1.2fr)_minmax(7.5rem,.7fr)_5.5rem_minmax(13rem,1.2fr)_5.5rem] gap-3 border-b border-border px-2 pb-2 text-2xs font-medium text-muted-foreground max-sm:grid-cols-[minmax(0,1fr)_minmax(11rem,1fr)]">
                <span>Branch</span>
                <span className="max-sm:hidden">Status</span>
                <span className="max-sm:hidden">Graph</span>
                <span>Copies</span>
                <span className="text-right max-sm:hidden">Changed</span>
              </div>
            )}
            <div className="divide-y divide-border">
              {visible.map((row) => {
                const places = locationsOf(row)
                const rowSelected = places.some((where) => checked.has(locationKey(row.name, where)))
                return (
                  <RowMenu
                    key={row.name}
                    row={row}
                    remotes={remoteList}
                    repoId={repo?.id ?? null}
                    busy={busy}
                  >
                    <div
                      className={cn(
                        'grid grid-cols-[minmax(0,1.2fr)_minmax(7.5rem,.7fr)_5.5rem_minmax(13rem,1.2fr)_5.5rem] items-center gap-3 px-2 py-2.5 transition-colors max-sm:grid-cols-[minmax(0,1fr)_minmax(11rem,1fr)]',
                        rowSelected ? 'bg-accent-text/8' : 'hover:bg-panel3/60',
                      )}
                    >
                      <div className="flex min-w-0 items-center gap-2">
                        <span className="min-w-0 overflow-hidden text-ellipsis whitespace-nowrap font-mono text-xs">
                          {row.name}
                        </span>
                        {row.isCurrent && (
                          <span className="flex-none rounded-sm bg-accent-text/15 px-1.5 py-0.5 text-2xs font-medium text-accent-text">
                            Open now
                          </span>
                        )}
                      </div>
                      <div className="max-sm:hidden">
                        <BranchStatus row={row} />
                      </div>
                      <Button
                        variant="ghost"
                        size="sm"
                        className={cn(
                          'h-7 justify-start gap-1.5 px-1.5 text-2xs max-sm:hidden',
                          !isBranchVisible(visibility, row.name) && 'text-muted-foreground',
                        )}
                        aria-pressed={isBranchVisible(visibility, row.name)}
                        aria-label={`${isBranchVisible(visibility, row.name) ? 'Hide' : 'Show'} ${row.name} in the graph`}
                        onClick={() => toggleGraphVisibility(row.name)}
                      >
                        {isBranchVisible(visibility, row.name) ? (
                          <Eye aria-hidden size={12} />
                        ) : (
                          <EyeOff aria-hidden size={12} />
                        )}
                        {isBranchVisible(visibility, row.name) ? 'Visible' : 'Hidden'}
                      </Button>
                      {/* One checkbox per copy. A branch on this computer and two
                          remotes is three things that can be deleted separately,
                          and a single row-level tick could only ever mean all. */}
                      <div className="flex flex-wrap items-center gap-1.5">
                        {places.map((where) => {
                          const key = locationKey(row.name, where)
                          const isChecked = checked.has(key)
                          const locked = where === 'local' && row.isCurrent
                          const label = where === 'local' ? 'This computer' : `Shared (${where})`
                          if (locked) {
                            return (
                              <LocationMenu
                                key={key}
                                row={row}
                                where={where}
                                remotes={remoteList}
                                repoId={repo?.id ?? null}
                                busy={busy}
                                tooltip="This branch is open now, so its copy on this computer cannot be deleted"
                              >
                                <span
                                  onContextMenu={(event) => event.stopPropagation()}
                                  className="inline-flex items-center gap-1.5 rounded-[5px] border border-border/60 bg-panel3/40 px-2 py-1 text-2xs text-muted-foreground"
                                >
                                  <Monitor aria-hidden size={11} />
                                  This computer
                                  <Lock aria-hidden size={10} />
                                </span>
                              </LocationMenu>
                            )}
                          return (
                            <LocationMenu
                              key={key}
                              row={row}
                              where={where}
                              remotes={remoteList}
                              repoId={repo?.id ?? null}
                              busy={busy}
                            >
                              <label
                                onContextMenu={(event) => event.stopPropagation()}
                                className={cn(
                                  'inline-flex cursor-pointer select-none items-center gap-1.5 rounded-[5px] border border-border px-2 py-1 text-2xs outline-none hover:bg-panel3 focus-within:border-accent-text/60 focus-within:ring-2 focus-within:ring-accent-text/20',
                                  isChecked && 'border-accent-text/50 bg-accent-text/10 text-foreground',
                                )}
                              >
                                <input
                                  type="checkbox"
                                  checked={isChecked}
                                  aria-label={`${row.name} on ${label}`}
                                  onChange={() => {}}
                                  onClick={(e) => {
                                    toggle(row, where, e.shiftKey)
                                  }}
                                  className="size-3.5 flex-none accent-[var(--gw-accent)] outline-none"
                                />
                                {where === 'local' ? (
                                  <Monitor aria-hidden size={11} className="flex-none" />
                                ) : (
                                  <Cloud aria-hidden size={11} className="flex-none" />
                                )}
                                {label}
                              </label>
                            </LocationMenu>
                          )
                        })}
                      </div>
                      <span className="text-right text-2xs text-muted-foreground max-sm:hidden">
                        {row.time != null ? formatRelativeTime(row.time) : 'Unknown'}
                      </span>
                    </div>
                  </RowMenu>
                )
              })}
            </div>
          </div>

          <div className={cn(
            'flex flex-wrap items-center gap-2 border-t border-border px-5 py-3 transition-colors',
            selected.length > 0 && 'bg-accent-text/5',
          )}>
            <div className="min-w-0">
              <div className={cn('text-xs font-medium', selected.length === 0 && 'text-muted-foreground')}>
                {/* While a bulk run is going, the count of what is left matters
                    far more than what is still ticked -- several pushes in a
                    row look identical to a hang without it. */}
                {progress
                  ? `${progress.done + 1} of ${progress.total}: ${progress.current ?? ''}`
                  : selected.length > 0
                    ? `${plural(selected.length, 'copy', 'copies')} selected across ${plural(selectedBranchCount, 'branch')}`
                    : 'Select copies to see what you can do'}
              </div>
              {progress && (
                <div
                  role="progressbar"
                  aria-valuemin={0}
                  aria-valuemax={progress.total}
                  aria-valuenow={progress.done}
                  className="mt-1 h-1 w-40 overflow-hidden rounded-full bg-panel3"
                >
                  <div
                    className="h-full rounded-full bg-accent-text transition-[width] duration-200"
                    style={{ width: `${Math.round((progress.done / Math.max(progress.total, 1)) * 100)}%` }}
                  />
                </div>
              )}
              {selected.length > 0 && (
                <div className="mt-0.5 text-2xs text-muted-foreground">
                  {localCount > 0 && `${plural(localCount, 'copy', 'copies')} on this computer`}
                  {localCount > 0 && serverCount > 0 && ', '}
                  {serverCount > 0 && `${plural(serverCount, 'shared copy', 'shared copies')}`}
                </div>
              )}
            </div>
            <div className="ml-auto flex items-center gap-2">
              {(availableActions.copyTargets.length > 0 || availableActions.copyAmbiguous) && (
                <DisabledHint
                  disabled={availableActions.copyAmbiguous}
                  reason="Choose only one shared copy for each branch. A local branch can only be linked to one shared copy."
                >
                  <Button
                    variant="secondary"
                    size="sm"
                    onClick={doCopyHere}
                    disabled={busy || availableActions.copyAmbiguous}
                  >
                    {m.copyRemoteBranchesMany.isPending
                      ? 'Adding…'
                      : `Get on this computer (${availableActions.copyTargets.length})`}
                  </Button>
                </DisabledHint>
              )}
              {pullable.length > 0 && (
                <Button
                  variant="secondary"
                  size="sm"
                  onClick={doPull}
                  disabled={busy}
                >
                  {m.pullBranchesMany.isPending ? 'Getting…' : `Get latest (${pullable.length})`}
                </Button>
              )}
              {sendable.length > 0 && (
                <Button variant="secondary" size="sm" onClick={doSend} disabled={busy}>
                  {m.pushBranchesMany.isPending ? 'Sending…' : `Send (${sendable.length})`}
                </Button>
              )}
              {selected.length > 0 && (
                <Button
                  variant="destructive"
                  size="sm"
                  onClick={() => setConfirmDelete(true)}
                  disabled={busy}
                >
                  Delete ({selected.length})
                </Button>
              )}
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
              {plural(serverCount, 'shared copy', 'shared copies')} will be removed, so
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
