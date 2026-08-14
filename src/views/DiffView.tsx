import { useEffect, useMemo, useState } from 'react'
import { useFileDiff, useStatus } from '@/hooks/useGitQueries'
import { useGitMutations } from '@/hooks/useGitMutations'
import { PendingIndicator } from '@/components/ui/pending-indicator'
import { useUiStore } from '@/stores/uiStore'
import { useActiveRepo } from '@/stores/workspaceStore'
import { FileHeader } from '@/components/domain/diff/FileHeader'
import { DiffLineRow } from '@/components/domain/diff/DiffLineRow'
import { DiffLineMenu } from '@/components/domain/diff/DiffLineMenu'
import { HunkBar } from '@/components/domain/diff/HunkBar'
import { LineSelectionBar } from '@/components/domain/diff/LineSelectionBar'
import type { DiffLineEntry, SelectedLine } from '@/lib/bindings'
import { computeWordSpans } from '@/lib/wordDiff'
import { WrapText } from 'lucide-react'
import { TooltipButton } from '@/components/ui/tooltip'
import { cn } from '@/lib/utils'

const WRAP_DIFF_LINES_KEY = 'gitwyrm:wrap-diff-lines'

function savedWrapPreference(): boolean {
  try {
    return window.localStorage.getItem(WRAP_DIFF_LINES_KEY) === 'true'
  } catch {
    return false
  }
}

/** Stable key for a changed line within a file diff. */
function lineKey(l: DiffLineEntry): string {
  return `${l.hunk_index}:${l.sign}:${l.old_no ?? ''}:${l.new_no ?? ''}`
}

function isChanged(l: DiffLineEntry): boolean {
  return l.sign === '+' || l.sign === '-'
}

function toSelected(l: DiffLineEntry): SelectedLine {
  return { hunk_index: l.hunk_index, old_no: l.old_no, new_no: l.new_no }
}

export function DiffView() {
  const repo = useActiveRepo()
  const request = useUiStore((s) => s.diffRequest)
  const closeDiff = useUiStore((s) => s.closeDiff)
  const diff = useFileDiff(repo?.id ?? null, request?.path ?? null, request?.source ?? null)
  const status = useStatus(repo?.id ?? null)
  const m = useGitMutations(repo?.id ?? null)
  const [wrapDiffLines, setWrapDiffLines] = useState(savedWrapPreference)

  const toggleWrap = () => {
    const next = !wrapDiffLines
    setWrapDiffLines(next)
    try {
      window.localStorage.setItem(WRAP_DIFF_LINES_KEY, String(next))
    } catch {
      // The visible control still works for this session when storage is unavailable.
    }
  }

  // Selected changed-line keys, local to this file view.
  const [selected, setSelected] = useState<Set<string>>(new Set())
  // Anchor for shift-click range selection.
  const [anchor, setAnchor] = useState<number | null>(null)
  // Key of the line whose right-click menu is open (the Semi-Active state).
  const [contextLine, setContextLine] = useState<string | null>(null)

  const lines = diff.data?.lines ?? []
  const kind = request?.source.kind

  // Reset selection whenever the viewed file or its source changes; the line
  // keys are only meaningful for the diff they came from.
  const sourceKey = request ? `${request.path}::${JSON.stringify(request.source)}` : null
  useEffect(() => {
    setSelected(new Set())
    setAnchor(null)
    setContextLine(null)
  }, [sourceKey])

  // A working-tree diff outlives its file once that file is committed or
  // discarded: the request still points at a path with nothing left to show, so
  // the view sits there empty. Drop back to the graph as soon as the path stops
  // being a pending change. Commit diffs are historical and always have content.
  const isWorkingTree = kind === 'staged' || kind === 'unstaged'
  const stillPending =
    !isWorkingTree ||
    !status.data ||
    status.data.staged.some((f) => f.path === request?.path) ||
    status.data.unstaged.some((f) => f.path === request?.path)
  useEffect(() => {
    if (!stillPending) closeDiff()
  }, [stillPending, closeDiff])

  // Only working-tree diffs are partially stageable; commit diffs are read-only.
  const canPatch = kind === 'staged' || kind === 'unstaged'

  // Which parts of each edited line actually changed, so the view can highlight
  // them instead of making the reader compare two long lines by eye.
  const wordSpans = useMemo(() => computeWordSpans(lines), [lines])

  // Index of every changed line, for range selection.
  const changedIndices = useMemo(
    () => lines.map((l, i) => (isChanged(l) ? i : -1)).filter((i) => i >= 0),
    [lines]
  )

  if (!request) return null

  const path = request.path

  const clearSelection = () => {
    setSelected(new Set())
    setAnchor(null)
  }

  const toggleLine = (index: number, shift: boolean) => {
    const line = lines[index]
    if (!line || !isChanged(line)) return
    setSelected((prev) => {
      const next = new Set(prev)
      if (shift && anchor != null) {
        const [lo, hi] = anchor < index ? [anchor, index] : [index, anchor]
        for (let i = lo; i <= hi; i++) {
          if (isChanged(lines[i])) next.add(lineKey(lines[i]))
        }
      } else {
        const key = lineKey(line)
        if (next.has(key)) next.delete(key)
        else next.add(key)
      }
      return next
    })
    setAnchor(index)
  }

  const selectionFor = (predicate: (l: DiffLineEntry) => boolean): SelectedLine[] =>
    lines.filter((l) => isChanged(l) && predicate(l)).map(toSelected)

  // Within a run of consecutive changed lines (no context between them), a
  // selected addition is anchored by whatever precedes it once the patch is
  // rebuilt. Unselected deletions ahead of it get demoted to context, which
  // pushes the addition past the lines it was meant to replace -- the patch
  // applies but stages the wrong thing. So a selection is only widened as far
  // as that hazard requires:
  //   - picking a deletion alone is always fine (context holds its position);
  //   - picking an addition pulls in any earlier deletions in the same run, so
  //     nothing is left to displace it.
  // "Removed a line, added lines right after it" therefore stays splittable:
  // the deletion stages on its own, and taking the additions only pulls in that
  // one deletion rather than the whole block.
  const expandedSelection = (keys: Set<string>): SelectedLine[] => {
    const out: SelectedLine[] = []
    let i = 0
    while (i < lines.length) {
      if (!isChanged(lines[i])) {
        i++
        continue
      }
      // [i, j) is a maximal contiguous run of changed lines.
      let j = i
      while (j < lines.length && isChanged(lines[j])) j++

      // The last selected addition in the run, if any: every deletion before it
      // must come along so it cannot be displaced.
      let lastSelectedAdd = -1
      for (let k = i; k < j; k++) {
        if (lines[k].sign === '+' && keys.has(lineKey(lines[k]))) lastSelectedAdd = k
      }

      for (let k = i; k < j; k++) {
        const picked =
          keys.has(lineKey(lines[k])) || (lines[k].sign === '-' && k < lastSelectedAdd)
        if (picked) out.push(toSelected(lines[k]))
      }
      i = j
    }
    return out
  }

  // Lines a right-click on `line` acts on: the whole current selection when the
  // clicked line is part of it, otherwise just that line's contiguous run.
  const contextTargetCount = (line: DiffLineEntry): number => {
    if (selected.has(lineKey(line)) && selected.size > 0) {
      return expandedSelection(selected).length
    }
    return expandedSelection(new Set([lineKey(line)])).length
  }

  const contextSelection = (line: DiffLineEntry): SelectedLine[] => {
    if (selected.has(lineKey(line)) && selected.size > 0) {
      return expandedSelection(selected)
    }
    return expandedSelection(new Set([lineKey(line)]))
  }

  const applyLine = (line: DiffLineEntry) => {
    const sel = contextSelection(line)
    if (sel.length === 0) return
    const args = { path, selection: sel }
    if (kind === 'staged') m.unstageLines.mutate(args, { onSuccess: clearSelection })
    else m.stageLines.mutate(args, { onSuccess: clearSelection })
  }

  const discardLine = (line: DiffLineEntry) => {
    const sel = contextSelection(line)
    if (sel.length === 0) return
    m.discardLines.mutate({ path, selection: sel }, { onSuccess: clearSelection })
  }

  const applyHunk = (hunkIndex: number) => {
    const sel = selectionFor((l) => l.hunk_index === hunkIndex)
    runPatch(sel)
  }

  const applySelected = () => {
    runPatch(expandedSelection(selected))
  }

  function runPatch(selection: SelectedLine[]) {
    if (selection.length === 0) return
    const args = { path, selection }
    if (kind === 'staged') m.unstageLines.mutate(args, { onSuccess: clearSelection })
    else m.stageLines.mutate(args, { onSuccess: clearSelection })
  }

  const discardSelected = () => {
    const sel = expandedSelection(selected)
    if (sel.length === 0) return
    m.discardLines.mutate({ path, selection: sel }, { onSuccess: clearSelection })
  }

  const discardHunk = (hunkIndex: number) => {
    const sel = selectionFor((l) => l.hunk_index === hunkIndex)
    if (sel.length === 0) return
    m.discardLines.mutate({ path, selection: sel }, { onSuccess: clearSelection })
  }

  const patchPending = m.stageLines.isPending || m.unstageLines.isPending || m.discardLines.isPending
  const patchLabel = m.discardLines.isPending
    ? 'Discarding selected lines…'
    : kind === 'staged'
      ? 'Unstaging selected lines…'
      : 'Staging selected lines…'

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <FileHeader
        request={request}
        additions={diff.data?.additions ?? 0}
        deletions={diff.data?.deletions ?? 0}
      >
        <TooltipButton
          onClick={toggleWrap}
          tooltip={wrapDiffLines ? 'Stop wrapping long lines' : 'Wrap long lines'}
          aria-pressed={wrapDiffLines}
          className={cn(
            'flex size-6 flex-none items-center justify-center rounded-[5px] border text-xs',
            wrapDiffLines
              ? 'border-border bg-soft text-accent-text'
              : 'border-border bg-panel2 text-sub hover:border-muted-foreground hover:bg-panel3'
          )}
        >
          <WrapText size={12} />
        </TooltipButton>
      </FileHeader>
      {patchPending && (
        <div className="flex h-7 flex-none items-center gap-2 border-b border-primary/25 bg-soft px-3 text-2xs font-medium text-accent-text" role="status">
          <PendingIndicator />
          {patchLabel}
        </div>
      )}
      <div className="min-h-0 flex-1 overflow-auto pb-5 font-mono text-xs leading-[1.8]">
        {diff.isLoading && <div className="p-4 text-xs text-muted-foreground">Loading diff…</div>}
        {diff.isError && (
          <div className="p-4 text-xs text-removed">{(diff.error as Error).message}</div>
        )}
        {diff.data?.binary && (
          <div className="p-4 text-xs text-muted-foreground">Binary file — no text diff.</div>
        )}
        {diff.data && !diff.data.binary && lines.length === 0 && (
          <div className="p-4 text-xs text-muted-foreground">No changes to show.</div>
        )}
        {lines.map((line, i) =>
          line.sign === '@' ? (
            <HunkBar
              key={i}
              text={line.text}
              canPatch={canPatch && !diff.data?.binary}
              kind={kind === 'staged' ? 'staged' : 'unstaged'}
              disabled={patchPending}
              onApply={() => applyHunk(line.hunk_index)}
              onDiscard={kind === 'unstaged' ? () => discardHunk(line.hunk_index) : undefined}
            />
          ) : canPatch && !diff.data?.binary && isChanged(line) ? (
            <DiffLineMenu
              key={i}
              kind={kind === 'staged' ? 'staged' : 'unstaged'}
              count={contextTargetCount(line)}
              disabled={patchPending}
              onOpenChange={(open) => setContextLine(open ? lineKey(line) : null)}
              onApply={() => applyLine(line)}
              onDiscard={() => discardLine(line)}
            >
              <DiffLineRow
                line={line}
                wordSpans={wordSpans.get(i)}
                selectable
                selected={selected.has(lineKey(line))}
                contextActive={contextLine === lineKey(line)}
                onSelect={(shift) => toggleLine(i, shift)}
                wrap={wrapDiffLines}
              />
            </DiffLineMenu>
          ) : (
            <DiffLineRow key={i} line={line} wordSpans={wordSpans.get(i)} wrap={wrapDiffLines} />
          )
        )}
      </div>
      {canPatch && selected.size > 0 && (
        <LineSelectionBar
          count={selected.size}
          kind={kind === 'staged' ? 'staged' : 'unstaged'}
          disabled={patchPending}
          onApply={applySelected}
          onDiscard={kind === 'unstaged' ? discardSelected : undefined}
          onClear={clearSelection}
        />
      )}
    </div>
  )
}
