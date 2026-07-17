import { useEffect, useState } from 'react'
import { Check, FileWarning } from 'lucide-react'
import { cn } from '@/lib/utils'
import { Button } from '@/components/ui/button'
import { useConflict, useMergeState } from '@/hooks/useGitQueries'
import { useGitMutations } from '@/hooks/useGitMutations'
import { useUiStore } from '@/stores/uiStore'
import { useActiveRepo } from '@/stores/workspaceStore'

/** Read-only side panel showing one version's text. */
function SidePanel({
  title,
  tone,
  text,
}: {
  title: string
  tone: 'ours' | 'theirs'
  text: string
}) {
  return (
    <div className="flex min-h-0 min-w-0 flex-1 flex-col border-r border-border last:border-r-0">
      <div
        className={cn(
          'flex-none border-b border-border px-3 py-1.5 text-[10px] font-bold tracking-[.05em]',
          tone === 'ours' ? 'text-added' : 'text-modified'
        )}
      >
        {title}
      </div>
      <pre className="min-h-0 flex-1 overflow-auto whitespace-pre-wrap break-words px-3 py-2 font-mono text-[11px] leading-[1.7] text-sub">
        {text || <span className="italic text-muted-foreground">(empty)</span>}
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

  // Load the working-tree (marker) text into the editable draft on file change.
  useEffect(() => {
    if (conflict.data) setDraft(conflict.data.merged)
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
          // Advance to the next unresolved file, or back to the graph.
          const next = conflicts.find((c) => c !== path)
          if (next) openConflict(next)
          else showGraph()
        },
      }
    )
  }

  if (!repo) return null

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

  return (
    <div className="flex min-h-0 flex-1">
      {/* Conflict file list */}
      <div className="w-56 flex-none overflow-y-auto border-r border-border bg-panel py-2">
        <div className="px-3 pb-1.5 text-[10px] font-bold tracking-[.05em] text-sub">
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
                'overflow-hidden text-ellipsis whitespace-nowrap text-[11.5px]',
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
          <span className="min-w-0 flex-1 overflow-hidden text-ellipsis whitespace-nowrap font-mono text-[11.5px] text-foreground">
            {path}
          </span>
          <Button
            size="sm"
            variant="secondary"
            className="h-7 gap-1.5 text-[11px] text-added"
            disabled={m.resolveConflict.isPending || !data}
            onClick={() => resolveWith({ kind: 'ours' })}
          >
            Use ours
          </Button>
          <Button
            size="sm"
            variant="secondary"
            className="h-7 gap-1.5 text-[11px] text-modified"
            disabled={m.resolveConflict.isPending || !data}
            onClick={() => resolveWith({ kind: 'theirs' })}
          >
            Use theirs
          </Button>
          <Button
            size="sm"
            className="h-7 gap-1.5 text-[11px]"
            disabled={m.resolveConflict.isPending || !data}
            onClick={() => resolveWith({ kind: 'manual', text: draft })}
          >
            <Check size={13} />
            Mark resolved
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
            Binary file — choose “Use ours” or “Use theirs”.
          </div>
        )}

        {data && !data.binary && (
          <div className="flex min-h-0 flex-1 flex-col">
            {/* Ours / Theirs reference panes */}
            <div className="flex min-h-0 flex-1 border-b border-border">
              <SidePanel title="OURS (current)" tone="ours" text={data.ours} />
              <SidePanel title="THEIRS (incoming)" tone="theirs" text={data.theirs} />
            </div>
            {/* Editable merged result */}
            <div className="flex min-h-0 flex-[1.2] flex-col">
              <div className="flex-none border-b border-border bg-panel2 px-3 py-1.5 text-[10px] font-bold tracking-[.05em] text-sub">
                RESULT — edit to resolve, then “Mark resolved”
              </div>
              <textarea
                value={draft}
                onChange={(e) => setDraft(e.target.value)}
                spellCheck={false}
                className="min-h-0 flex-1 resize-none bg-background px-3 py-2 font-mono text-[11.5px] leading-[1.7] text-foreground outline-none"
              />
            </div>
          </div>
        )}
      </div>
    </div>
  )
}
