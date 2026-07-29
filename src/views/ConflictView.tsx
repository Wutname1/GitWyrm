import { useEffect, useMemo, useRef, useState } from 'react'
import { Check, FileWarning } from 'lucide-react'
import { cn } from '@/lib/utils'
import { Button } from '@/components/ui/button'
import { PendingIndicator } from '@/components/ui/pending-indicator'
import { ResizeHandle } from '@/components/ui/ResizeHandle'
import { useConflict, useMergeState } from '@/hooks/useGitQueries'
import { useGitMutations } from '@/hooks/useGitMutations'
import { useUiStore } from '@/stores/uiStore'
import {
  DEFAULT_CONFLICT_RESULT_SPLIT,
  DEFAULT_CONFLICT_SIDE_SPLIT,
  MAX_CONFLICT_RESULT_SPLIT,
  MAX_CONFLICT_SIDE_SPLIT,
  MIN_CONFLICT_RESULT_SPLIT,
  MIN_CONFLICT_SIDE_SPLIT,
  useActiveRepo,
  useWorkspaceStore,
} from '@/stores/workspaceStore'

/** Shared type ramp so gutter digits and code sit on the same baseline. */
const CODE_LINE = 'font-mono text-2xs leading-[1.7]'

/** Read-only side panel showing one version's text, with line numbers. */
function SidePanel({
  title,
  tone,
  text,
  deleted,
}: {
  title: string
  tone: 'ours' | 'theirs'
  text: string
  deleted: boolean
}) {
  const lines = useMemo(() => text.split('\n'), [text])

  return (
    <div className="flex min-h-0 min-w-0 flex-1 flex-col">
      <div
        className={cn(
          'flex-none border-b border-border px-3 py-1.5 text-2xs font-bold tracking-[.05em]',
          tone === 'ours' ? 'text-added' : 'text-modified'
        )}
      >
        {title}
      </div>
      {deleted ? (
        <div className={cn('min-h-0 flex-1 overflow-auto px-3 py-2 italic text-removed', CODE_LINE)}>
          This side deleted the file. Choosing it removes the file.
        </div>
      ) : text === '' ? (
        <div
          className={cn(
            'min-h-0 flex-1 overflow-auto px-3 py-2 italic text-muted-foreground',
            CODE_LINE
          )}
        >
          (empty)
        </div>
      ) : (
        // One scroll container holding a sticky gutter, so the numbers stay put
        // horizontally while long lines scroll under them.
        <div className="min-h-0 flex-1 overflow-auto py-2">
          <div className="flex min-w-max">
            <div
              aria-hidden
              className={cn(
                'sticky left-0 z-10 flex-none select-none bg-background px-2 text-right text-muted-foreground',
                CODE_LINE
              )}
            >
              {lines.map((_, i) => (
                <div key={i}>{i + 1}</div>
              ))}
            </div>
            <div className={cn('flex-1 whitespace-pre pr-3 text-sub', CODE_LINE)}>
              {lines.map((line, i) => (
                <div key={i}>{line === '' ? ' ' : line}</div>
              ))}
            </div>
          </div>
        </div>
      )}
    </div>
  )
}

/**
 * Line-number gutter for the editable RESULT pane. A textarea can't render its
 * own gutter, so this mirrors it in a sibling column kept in vertical sync by
 * the textarea's scroll handler.
 */
function ResultGutter({ count, scrollTop }: { count: number; scrollTop: number }) {
  return (
    <div
      aria-hidden
      className="relative flex-none select-none overflow-hidden border-r border-border bg-panel/40 px-2 py-2 text-right"
    >
      <div className={cn('text-muted-foreground', CODE_LINE)} style={{ transform: `translateY(${-scrollTop}px)` }}>
        {Array.from({ length: count }, (_, i) => (
          <div key={i}>{i + 1}</div>
        ))}
      </div>
    </div>
  )
}

export function ConflictView() {
  const repo = useActiveRepo()
  const path = useUiStore((s) => s.conflictPath)
  const openConflict = useUiStore((s) => s.openConflict)
  const showGraph = useUiStore((s) => s.showGraph)

  const merge = useMergeState(repo?.id ?? null)
  const conflict = useConflict(repo?.id ?? null, path)
  const m = useGitMutations(repo?.id ?? null)

  const conflicts = merge.data?.conflicts ?? []
  const [draft, setDraft] = useState('')

  const conflictSideSplit = useWorkspaceStore((s) => s.conflictSideSplit)
  const setConflictSideSplit = useWorkspaceStore((s) => s.setConflictSideSplit)
  const conflictResultSplit = useWorkspaceStore((s) => s.conflictResultSplit)
  const setConflictResultSplit = useWorkspaceStore((s) => s.setConflictResultSplit)

  const hSplitRef = useRef<HTMLDivElement>(null)
  const vSplitRef = useRef<HTMLDivElement>(null)

  /** Drag distance in pixels as a share of the width OURS and THEIRS split. */
  const pixelsToSideSplit = (pixels: number) => {
    const width = hSplitRef.current?.clientWidth ?? 0
    return width > 0 ? (pixels / width) * 100 : 0
  }

  /** Drag distance in pixels as a share of the height the rows split. */
  const pixelsToResultSplit = (pixels: number) => {
    const height = vSplitRef.current?.clientHeight ?? 0
    return height > 0 ? (pixels / height) * 100 : 0
  }

  // The RESULT gutter can't scroll itself; it mirrors the textarea's offset.
  const [resultScrollTop, setResultScrollTop] = useState(0)
  const draftLineCount = useMemo(() => draft.split('\n').length, [draft])

  // Load the marker text into the editable draft only when the shown file
  // changes, so a background refetch never wipes in-progress hand edits.
  const loadedPath = useRef<string | null>(null)
  useEffect(() => {
    if (conflict.data && conflict.data.path !== loadedPath.current) {
      loadedPath.current = conflict.data.path
      setDraft(conflict.data.merged)
    }
  }, [conflict.data])

  // When no path is selected but conflicts exist, jump to the first one.
  useEffect(() => {
    if (!path && conflicts.length > 0) openConflict(conflicts[0])
  }, [path, conflicts, openConflict])

  const resolveWith = (resolution: Parameters<typeof m.resolveConflict.mutate>[0]['resolution']) => {
    if (!path) return
    m.resolveConflict.mutate(
      { path, resolution },
      {
        onSuccess: () => {
          // Advance to the file after this one (wrapping), or back to the graph.
          const remaining = conflicts.filter((c) => c !== path)
          if (remaining.length === 0) {
            showGraph()
            return
          }
          const idx = conflicts.indexOf(path)
          const next = remaining.find((c) => conflicts.indexOf(c) > idx) ?? remaining[0]
          openConflict(next)
        },
      }
    )
  }

  if (!repo) return null

  // Merge state not known yet: render nothing rather than flashing the
  // "no conflicts" screen for a repo that may be mid-operation.
  if (merge.isLoading) return null

  if (conflicts.length === 0) {
    return (
      <div className="flex min-h-0 flex-1 flex-col items-center justify-center gap-2 text-center">
        <Check size={28} className="text-added" />
        <div className="text-sm font-medium text-foreground">No conflicts to resolve</div>
        <div className="text-xs text-muted-foreground">
          Every conflicted file has been handled.
        </div>
        <Button variant="secondary" size="sm" className="mt-2" onClick={showGraph}>
          Back to graph
        </Button>
      </div>
    )
  }

  const data = conflict.data
  const activeResolution = m.resolveConflict.isPending
    ? m.resolveConflict.variables?.resolution.kind
    : undefined

  return (
    <div className="flex min-h-0 flex-1">
      {/* Conflict file list */}
      <div className="w-56 flex-none overflow-y-auto border-r border-border bg-panel py-2">
        <div className="px-3 pb-1.5 text-2xs font-bold tracking-[.05em] text-sub">
          CONFLICTED FILES
        </div>
        {conflicts.map((c) => (
          <button
            key={c}
            onClick={() => openConflict(c)}
            className={cn(
              'flex w-full items-center gap-2 px-3 py-1.5 text-left hover:bg-panel2',
              c === path && 'bg-soft'
            )}
          >
            <FileWarning size={13} className="flex-none text-removed" />
            <span
              className={cn(
                'overflow-hidden text-ellipsis whitespace-nowrap text-xs',
                c === path ? 'font-semibold text-foreground' : 'text-sub'
              )}
            >
              {c.split('/').pop()}
            </span>
          </button>
        ))}
      </div>

      {/* Resolution area */}
      <div className="flex min-h-0 min-w-0 flex-1 flex-col">
        <div className="flex flex-none items-center gap-2 border-b border-border bg-panel px-3.5 py-2">
          <FileWarning size={14} className="flex-none text-removed" />
          <span className="min-w-0 flex-1 overflow-hidden text-ellipsis whitespace-nowrap font-mono text-xs text-foreground">
            {path}
          </span>
          <Button
            size="sm"
            variant="secondary"
            className="h-7 gap-1.5 text-2xs text-added"
            disabled={m.resolveConflict.isPending || !data}
            onClick={() => resolveWith({ kind: 'ours' })}
          >
            {activeResolution === 'ours' && <PendingIndicator />}
            {activeResolution === 'ours'
              ? 'Using ours…'
              : data?.ours_deleted
                ? 'Use ours (delete)'
                : 'Use ours'}
          </Button>
          <Button
            size="sm"
            variant="secondary"
            className="h-7 gap-1.5 text-2xs text-modified"
            disabled={m.resolveConflict.isPending || !data}
            onClick={() => resolveWith({ kind: 'theirs' })}
          >
            {activeResolution === 'theirs' && <PendingIndicator />}
            {activeResolution === 'theirs'
              ? 'Using theirs…'
              : data?.theirs_deleted
                ? 'Use theirs (delete)'
                : 'Use theirs'}
          </Button>
          <Button
            size="sm"
            className="h-7 gap-1.5 text-2xs"
            disabled={m.resolveConflict.isPending || !data || data.binary}
            onClick={() => resolveWith({ kind: 'manual', text: draft })}
          >
            {activeResolution === 'manual' ? <PendingIndicator /> : <Check size={13} />}
            {activeResolution === 'manual' ? 'Marking resolved…' : 'Mark resolved'}
          </Button>
        </div>

        {conflict.isLoading && (
          <div className="p-4 text-xs text-muted-foreground">Loading conflict…</div>
        )}
        {conflict.isError && (
          <div className="p-4 text-xs text-removed">{(conflict.error as Error).message}</div>
        )}
        {data?.binary && (
          <div className="p-4 text-xs text-muted-foreground">
            This file isn't text, so it can't be edited here — pick “Use ours” or “Use
            theirs” to keep one whole version.
          </div>
        )}

        {data && !data.binary && (
          <div ref={vSplitRef} className="flex min-h-0 flex-1 flex-col">
            {/* Ours / Theirs reference panes, split left-to-right */}
            <div
              ref={hSplitRef}
              className="flex min-h-0"
              style={{ flex: `${conflictResultSplit} 1 0%` }}
            >
              <div
                className="relative flex min-h-0 min-w-0 border-r border-border"
                style={{ flex: `${conflictSideSplit} 1 0%` }}
              >
                <SidePanel
                  title="OURS (current)"
                  tone="ours"
                  text={data.ours}
                  deleted={data.ours_deleted}
                />
                <ResizeHandle
                  ariaLabel="Resize ours and theirs panes"
                  value={conflictSideSplit}
                  min={MIN_CONFLICT_SIDE_SPLIT}
                  max={MAX_CONFLICT_SIDE_SPLIT}
                  defaultValue={DEFAULT_CONFLICT_SIDE_SPLIT}
                  onChange={setConflictSideSplit}
                  toValue={pixelsToSideSplit}
                  className="-right-1"
                />
              </div>
              <div
                className="flex min-h-0 min-w-0"
                style={{ flex: `${100 - conflictSideSplit} 1 0%` }}
              >
                <SidePanel
                  title="THEIRS (incoming)"
                  tone="theirs"
                  text={data.theirs}
                  deleted={data.theirs_deleted}
                />
              </div>
            </div>

            {/* Editable merged result */}
            <div
              className="relative flex min-h-0 flex-col border-t border-border"
              style={{ flex: `${100 - conflictResultSplit} 1 0%` }}
            >
              <ResizeHandle
                ariaLabel="Resize result pane"
                axis="y"
                direction={-1}
                value={100 - conflictResultSplit}
                min={100 - MAX_CONFLICT_RESULT_SPLIT}
                max={100 - MIN_CONFLICT_RESULT_SPLIT}
                defaultValue={100 - DEFAULT_CONFLICT_RESULT_SPLIT}
                onChange={(v) => setConflictResultSplit(100 - v)}
                toValue={pixelsToResultSplit}
                className="-top-1"
              />
              <div className="flex-none border-b border-border bg-panel2 px-3 py-1.5 text-2xs font-bold tracking-[.05em] text-sub">
                RESULT — edit to resolve, then “Mark resolved”
              </div>
              <div className="flex min-h-0 flex-1">
                <ResultGutter count={draftLineCount} scrollTop={resultScrollTop} />
                <textarea
                  value={draft}
                  onChange={(e) => setDraft(e.target.value)}
                  onScroll={(e) => setResultScrollTop(e.currentTarget.scrollTop)}
                  spellCheck={false}
                  wrap="off"
                  className={cn(
                    'min-h-0 flex-1 resize-none bg-background px-3 py-2 text-foreground outline-none',
                    CODE_LINE
                  )}
                />
              </div>
            </div>
          </div>
        )}
      </div>
    </div>
  )
}
