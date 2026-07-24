import { useEffect, useMemo, useRef } from 'react'
import { useVirtualizer } from '@tanstack/react-virtual'
import type { CommitEntry, StashInfo } from '@/lib/bindings'
import { GRAPH_ROW_HEIGHT, GRAPH_ROW_WITH_CHANGES_HEIGHT } from '@/lib/gitDisplay'
import { useCommitLog, useStashes, useStatus } from '@/hooks/useGitQueries'
import { useGraphLoadSpan } from '@/lib/perf'
import { useUiStore } from '@/stores/uiStore'
import { useActiveRepo, useWorkspaceStore } from '@/stores/workspaceStore'
import { CommitRow } from '@/components/domain/graph/CommitRow'
import { PendingRow } from '@/components/domain/graph/PendingRow'
import { StashRow } from '@/components/domain/graph/StashRow'
import { GraphSvg, type GraphRow } from '@/components/domain/graph/GraphSvg'
import { GraphHeader } from '@/components/domain/graph/GraphHeader'
import { NoRepoPlaceholder } from '@/components/domain/NoRepoPlaceholder'
import {
  columnWidth,
  effectiveHiddenColumns,
  gridTemplate,
  totalColumnsWidth,
  visibleColumns,
} from '@/lib/graphColumns'

/** Sentinel selection value for the synthetic WIP row (not a real commit). */
export const WIP_SHA = '__wip__'

export function GraphView() {
  const repo = useActiveRepo()
  const selectedSha = useUiStore((s) => s.selectedSha)
  const selectCommit = useUiStore((s) => s.selectCommit)
  const focusChanges = useUiStore((s) => s.focusChanges)
  const revealRef = useUiStore((s) => s.revealRef)
  const revealSha = useUiStore((s) => s.revealSha)
  const columnOrder = useWorkspaceStore((s) => s.columnOrder)
  const hiddenColumns = useWorkspaceStore((s) => s.hiddenColumns)
  const columnWidths = useWorkspaceStore((s) => s.columnWidths)
  const changeSizeDisplay = useWorkspaceStore((s) => s.changeSizeDisplay)
  const showChangeIndicator = useWorkspaceStore((s) => s.showChangeIndicator)
  const effectiveHidden = effectiveHiddenColumns(hiddenColumns, showChangeIndicator, changeSizeDisplay)
  const visible = visibleColumns(columnOrder, effectiveHidden)
  const graphColumnIndex = visible.indexOf('graph')
  const graphWidth = columnWidth('graph', columnWidths)
  const graphGridTemplate = gridTemplate(columnOrder, effectiveHidden, columnWidths)
  const columnsWidth = totalColumnsWidth(columnOrder, effectiveHidden, columnWidths)
  const rowHeight = showChangeIndicator && changeSizeDisplay === 'row'
    ? GRAPH_ROW_WITH_CHANGES_HEIGHT
    : GRAPH_ROW_HEIGHT
  const scrollRef = useRef<HTMLDivElement>(null)
  const headerRef = useRef<HTMLDivElement>(null)

  // The header sits outside the scroll container so it stays pinned vertically.
  // Mirror the body's horizontal offset onto it so columns stay lined up when
  // the graph is scrolled sideways.
  const handleScroll = () => {
    const header = headerRef.current
    if (header) header.scrollLeft = scrollRef.current?.scrollLeft ?? 0
  }

  const log = useCommitLog(repo?.id ?? null)
  const status = useStatus(repo?.id ?? null)
  const stashes = useStashes(repo?.id ?? null)

  // Time the felt "opened a repo -> graph is on screen" gap that happens
  // entirely after open_repo returns: the commit-log fetch plus first paint.
  // open_repo's own span covers the IPC; this covers everything after it, which
  // is the latency the user actually stares at. Keyed on repo id so switching
  // repos re-measures.
  useGraphLoadSpan(repo?.id ?? null, log.isLoading)
  const commits: CommitEntry[] = useMemo(
    () => log.data?.pages.flatMap((p) => p.commits) ?? [],
    [log.data]
  )

  const stagedCount = status.data?.staged.length ?? 0
  const unstagedCount = status.data?.unstaged.length ?? 0
  const pendingFiles = [...(status.data?.staged ?? []), ...(status.data?.unstaged ?? [])]
  const pendingFileCount = new Set(pendingFiles.map((file) => file.path)).size
  const pendingAdditions = pendingFiles.reduce((total, file) => total + file.additions, 0)
  const pendingDeletions = pendingFiles.reduce((total, file) => total + file.deletions, 0)
  const pending = commits.length > 0 && stagedCount + unstagedCount > 0
  const headCommitLoaded = commits.some((commit) =>
    commit.refs.some((ref) => ref.type === 'head')
  )

  // One unified row list: the WIP row first, then commits and stashes merged
  // by TIME, so a stash made five minutes ago sits at the top of the graph
  // even when the commit it was taken on is far down in history. The graph
  // svg draws a dashed connector from each stash down to its base commit, so
  // "when it was made" and "what it was based on" are both visible at once.
  const hasMorePages = log.hasNextPage ?? false
  const rows: GraphRow[] = useMemo(() => {
    const out: GraphRow[] = []
    if (pending) out.push({ kind: 'wip' })
    const sorted = [...(stashes.data ?? [])].sort((a, b) => b.time - a.time)
    const emitted = new Set<string>()
    const emit = (s: StashInfo) => {
      if (emitted.has(s.sha)) return
      emitted.add(s.sha)
      out.push({ kind: 'stash', stash: s })
    }
    for (const c of commits) {
      // Time rule: anything newer than this commit goes above it.
      for (const s of sorted) {
        if (s.time >= c.time) emit(s)
        else break
      }
      // Topology rule: a stash must sit ABOVE the commit it was taken on,
      // even when topo order puts an older-timed commit higher -- otherwise
      // its connector would point upward and read as a broken line.
      for (const s of sorted) if (s.base_sha === c.sha) emit(s)
      out.push({ kind: 'commit', commit: c })
    }
    // Stashes older than every loaded commit belong further down in history.
    // Hold them back until their time position pages in; once history is
    // exhausted, show whatever is left at the bottom.
    if (!hasMorePages) for (const s of sorted) emit(s)
    return out
  }, [commits, stashes.data, pending, hasMorePages])

  const virtualizer = useVirtualizer({
    count: rows.length,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => rowHeight,
    overscan: 12,
  })

  // Row mode changes every virtual row's height. Clear cached measurements so
  // switching the setting visibly reflows the graph immediately.
  useEffect(() => {
    virtualizer.measure()
  }, [rowHeight, virtualizer])

  const items = virtualizer.getVirtualItems()
  const startIndex = items.length ? items[0].index : 0
  const endIndex = items.length ? items[items.length - 1].index : 0

  // Fetch the next page when scrolling nears the end. Depend only on stable
  // primitives: `items` and `log` get fresh identities every render, so
  // including them would re-run this effect on every render and could spin
  // fetchNextPage in a loop.
  const lastVisibleIndex = items.length ? items[items.length - 1].index : 0
  const { hasNextPage, isFetchingNextPage, fetchNextPage } = log
  // WIP is a child of the checked-out branch tip. If that tip is old enough
  // to fall outside the first page, keep loading until its real endpoint is
  // available instead of attaching WIP to the newest visible commit.
  useEffect(() => {
    if (
      pending &&
      repo?.head_branch != null &&
      !headCommitLoaded &&
      hasNextPage &&
      !isFetchingNextPage
    ) {
      fetchNextPage()
    }
  }, [pending, repo?.head_branch, headCommitLoaded, hasNextPage, isFetchingNextPage, fetchNextPage])

  useEffect(() => {
    if (lastVisibleIndex >= rows.length - 40 && hasNextPage && !isFetchingNextPage) {
      fetchNextPage()
    }
  }, [lastVisibleIndex, rows.length, hasNextPage, isFetchingNextPage, fetchNextPage])

  // Reveal a branch/tag in the graph: scroll its tip commit into view and select
  // it. If the tip isn't in a loaded page yet, keep pulling pages until it turns
  // up or history is exhausted. `revealRef.nonce` re-triggers on repeat clicks.
  const revealNonce = revealRef?.nonce
  const revealName = revealRef?.name
  useEffect(() => {
    if (!revealName) return
    const index = rows.findIndex(
      (r) => r.kind === 'commit' && r.commit.refs.some((ref) => ref.name === revealName)
    )
    if (index >= 0) {
      const row = rows[index]
      if (row.kind === 'commit') selectCommit(row.commit.sha)
      virtualizer.scrollToIndex(index, { align: 'center' })
    } else if (hasNextPage && !isFetchingNextPage) {
      fetchNextPage()
    }
    // Depend on the nonce (repeat clicks) and on rows growing (so a pending
    // fetch that lands the tip re-runs this and scrolls once it's loaded).
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [revealNonce, revealName, rows.length])

  // Reveal a specific commit or stash by sha (sidebar stash click). Same
  // paging behavior as revealRef when the target row isn't loaded yet.
  const revealShaNonce = revealSha?.nonce
  const revealShaValue = revealSha?.sha
  useEffect(() => {
    if (!revealShaValue) return
    const index = rows.findIndex(
      (r) =>
        (r.kind === 'commit' && r.commit.sha === revealShaValue) ||
        (r.kind === 'stash' && r.stash.sha === revealShaValue)
    )
    if (index >= 0) {
      selectCommit(revealShaValue)
      virtualizer.scrollToIndex(index, { align: 'center' })
    } else if (hasNextPage && !isFetchingNextPage) {
      fetchNextPage()
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [revealShaNonce, revealShaValue, rows.length])

  if (!repo) return <NoRepoPlaceholder />

  if (log.isError) {
    return (
      <div className="flex flex-1 items-center justify-center text-sm text-removed">
        {(log.error as Error).message}
      </div>
    )
  }

  if (log.isLoading) {
    return (
      <div className="flex-1 space-y-px overflow-hidden pl-3 pt-2">
        {Array.from({ length: 18 }, (_, i) => (
          <div key={i} className="flex h-[27px] animate-pulse items-center gap-4">
            <div className="ml-[150px] size-3 rounded-full bg-panel3" />
            <div className="h-2.5 rounded bg-panel3" style={{ width: `${180 + ((i * 67) % 200)}px` }} />
            <div className="h-2.5 w-24 rounded bg-panel2" />
          </div>
        ))}
      </div>
    )
  }

  return (
    <>
      <GraphHeader scrollRef={headerRef} />

      <div
        ref={scrollRef}
        onScroll={handleScroll}
        className="relative min-h-0 flex-1 overflow-auto pl-3"
      >
        <div
          className="relative"
          style={{ height: virtualizer.getTotalSize(), minWidth: columnsWidth }}
        >
          {graphColumnIndex >= 0 && (
            <div
              aria-hidden="true"
              className="pointer-events-none absolute inset-x-0 top-0 grid"
              style={{
                height: virtualizer.getTotalSize(),
                gridTemplateColumns: graphGridTemplate,
              }}
            >
              <div className="min-w-0 overflow-hidden" style={{ gridColumn: graphColumnIndex + 1 }}>
                <GraphSvg
                  rows={rows}
                  selectedSha={selectedSha}
                  startIndex={startIndex}
                  endIndex={endIndex}
                  width={graphWidth}
                  rowHeight={rowHeight}
                />
              </div>
            </div>
          )}
          {items.map((vi) => {
            const rowStyle: React.CSSProperties = {
              position: 'absolute',
              top: 0,
              left: 0,
              right: 0,
              transform: `translateY(${vi.start}px)`,
            }
            const row = rows[vi.index]
            if (!row) return null
            if (row.kind === 'wip') {
              return (
                <PendingRow
                  key="__wip"
                  stagedCount={stagedCount}
                  unstagedCount={unstagedCount}
                  filesChanged={pendingFileCount}
                  additions={pendingAdditions}
                  deletions={pendingDeletions}
                  rowHeight={rowHeight}
                  selected={selectedSha === WIP_SHA}
                  onSelect={() => {
                    selectCommit(WIP_SHA)
                    focusChanges()
                  }}
                  style={rowStyle}
                />
              )
            }
            if (row.kind === 'stash') {
              const selected = selectedSha === row.stash.sha
              return (
                <StashRow
                  key={`stash:${row.stash.sha}`}
                  stash={row.stash}
                  selected={selected}
                  onSelect={() => selectCommit(selected ? null : row.stash.sha)}
                  rowHeight={rowHeight}
                  style={rowStyle}
                />
              )
            }
            const commit = row.commit
            const selected = selectedSha === commit.sha
            return (
              <CommitRow
                key={commit.sha}
                commit={commit}
                selected={selected}
                onSelect={() => selectCommit(selected ? null : commit.sha)}
                rowHeight={rowHeight}
                style={rowStyle}
              />
            )
          })}
        </div>
      </div>

    </>
  )
}
