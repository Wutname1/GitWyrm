import { useEffect, useMemo, useRef } from 'react'
import { useVirtualizer } from '@tanstack/react-virtual'
import type { CommitEntry } from '@/lib/bindings'
import { GRAPH_ROW_HEIGHT } from '@/lib/gitDisplay'
import { useCommitLog } from '@/hooks/useGitQueries'
import { useUiStore } from '@/stores/uiStore'
import { useActiveRepo } from '@/stores/workspaceStore'
import { CommitRow, GRAPH_GRID } from '@/components/domain/graph/CommitRow'
import { GraphSvg } from '@/components/domain/graph/GraphSvg'
import { CommitDrawer } from '@/components/domain/graph/CommitDrawer'
import { cn } from '@/lib/utils'

export function GraphView() {
  const repo = useActiveRepo()
  const selectedSha = useUiStore((s) => s.selectedSha)
  const selectCommit = useUiStore((s) => s.selectCommit)
  const scrollRef = useRef<HTMLDivElement>(null)

  const log = useCommitLog(repo?.id ?? null)
  const commits: CommitEntry[] = useMemo(
    () => log.data?.pages.flatMap((p) => p.commits) ?? [],
    [log.data]
  )

  const virtualizer = useVirtualizer({
    count: commits.length,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => GRAPH_ROW_HEIGHT,
    overscan: 12,
  })

  const items = virtualizer.getVirtualItems()
  const startIndex = items.length ? items[0].index : 0
  const endIndex = items.length ? items[items.length - 1].index : 0

  // Fetch the next page when scrolling nears the end.
  useEffect(() => {
    if (!items.length) return
    const last = items[items.length - 1]
    if (last.index >= commits.length - 40 && log.hasNextPage && !log.isFetchingNextPage) {
      log.fetchNextPage()
    }
  }, [items, commits.length, log])

  if (!repo) {
    return (
      <div className="flex flex-1 items-center justify-center text-sm text-muted-foreground">
        Open a repository to see its history
      </div>
    )
  }

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
      <div
        className={cn(
          'grid h-[30px] flex-none items-center border-b border-border pl-3 pr-1 text-[10px] font-bold tracking-[.06em] text-muted-foreground',
          GRAPH_GRID
        )}
      >
        <span>BRANCH / TAG</span>
        <span>GRAPH</span>
        <span>COMMIT MESSAGE</span>
        <span>AUTHOR</span>
        <span>DATE</span>
        <span>SHA</span>
      </div>

      <div ref={scrollRef} className="relative min-h-0 flex-1 overflow-auto pl-3">
        <div className="relative" style={{ height: virtualizer.getTotalSize() }}>
          <GraphSvg
            commits={commits}
            selectedSha={selectedSha}
            startIndex={startIndex}
            endIndex={endIndex}
          />
          {items.map((vi) => {
            const commit = commits[vi.index]
            if (!commit) return null
            const selected = selectedSha === commit.sha
            return (
              <CommitRow
                key={commit.sha}
                commit={commit}
                selected={selected}
                onSelect={() => selectCommit(selected ? null : commit.sha)}
                style={{
                  position: 'absolute',
                  top: 0,
                  left: 0,
                  right: 0,
                  transform: `translateY(${vi.start}px)`,
                }}
              />
            )
          })}
        </div>
      </div>

      {selectedSha != null && <CommitDrawer repoId={repo.id} sha={selectedSha} />}
    </>
  )
}
