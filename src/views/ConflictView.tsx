import { useEffect, useRef, useState } from 'react'
import { Check, FileWarning } from 'lucide-react'
import { cn } from '@/lib/utils'
import { Button } from '@/components/ui/button'
import { PendingIndicator } from '@/components/ui/pending-indicator'
import { useConflict, useMergeState } from '@/hooks/useGitQueries'
import { useGitMutations } from '@/hooks/useGitMutations'
import { useUiStore } from '@/stores/uiStore'
import { useActiveRepo } from '@/stores/workspaceStore'

/** Read-only side panel showing one version's text. */
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
  return (
    <div className="flex min-h-0 min-w-0 flex-1 flex-col border-r border-border last:border-r-0">
      <div
        className={cn(
          'flex-none border-b border-border px-3 py-1.5 text-2xs font-bold tracking-[.05em]',
          tone === 'ours' ? 'text-added' : 'text-modified'
        )}
      >
        {title}
      </div>
      <pre className="min-h-0 flex-1 overflow-auto whitespace-pre-wrap break-words px-3 py-2 font-mono text-2xs leading-[1.7] text-sub">
        {deleted ? (
          <span className="italic text-removed">
            This side deleted the file. Choosing it removes the file.
          </span>
        ) : (
          text || <span className="italic text-muted-foreground">(empty)</span>
        )}
      </pre>
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
          <div className="flex min-h-0 flex-1 flex-col">
            {/* Ours / Theirs reference panes */}
            <div className="flex min-h-0 flex-1 border-b border-border">
              <SidePanel
                title="OURS (current)"
                tone="ours"
                text={data.ours}
                deleted={data.ours_deleted}
              />
              <SidePanel
                title="THEIRS (incoming)"
                tone="theirs"
                text={data.theirs}
                deleted={data.theirs_deleted}
              />
            </div>
            {/* Editable merged result */}
            <div className="flex min-h-0 flex-[1.2] flex-col">
              <div className="flex-none border-b border-border bg-panel2 px-3 py-1.5 text-2xs font-bold tracking-[.05em] text-sub">
                RESULT — edit to resolve, then “Mark resolved”
              </div>
              <textarea
                value={draft}
                onChange={(e) => setDraft(e.target.value)}
                spellCheck={false}
                className="min-h-0 flex-1 resize-none bg-background px-3 py-2 font-mono text-xs leading-[1.7] text-foreground outline-none"
              />
            </div>
          </div>
        )}
      </div>
    </div>
  )
}
