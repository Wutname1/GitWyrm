import { useEffect, useMemo, useState } from 'react'
import { useFileDiff } from '@/hooks/useGitQueries'
import { useGitMutations } from '@/hooks/useGitMutations'
import { useUiStore } from '@/stores/uiStore'
import { useActiveRepo } from '@/stores/workspaceStore'
import { FileHeader } from '@/components/domain/diff/FileHeader'
import { DiffLineRow } from '@/components/domain/diff/DiffLineRow'
import { DiffLineMenu } from '@/components/domain/diff/DiffLineMenu'
import { HunkBar } from '@/components/domain/diff/HunkBar'
import { LineSelectionBar } from '@/components/domain/diff/LineSelectionBar'
import type { DiffLineEntry, SelectedLine } from '@/lib/bindings'

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
  const diff = useFileDiff(repo?.id ?? null, request?.path ?? null, request?.source ?? null)
  const m = useGitMutations(repo?.id ?? null)

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

  // Only working-tree diffs are partially stageable; commit diffs are read-only.
  const canPatch = kind === 'staged' || kind === 'unstaged'

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

  // A run of consecutive changed lines with no context line between them (e.g.
  // `-a -b +A +B`) can only be staged as a unit: git apply would misplace an
  // added line if only part of such a block is selected. So expand any
  // partially-selected contiguous change run to cover the whole run.
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
      let anySelected = false
      while (j < lines.length && isChanged(lines[j])) {
        if (keys.has(lineKey(lines[j]))) anySelected = true
        j++
      }
      if (anySelected) {
        for (let k = i; k < j; k++) out.push(toSelected(lines[k]))
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

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <FileHeader
        request={request}
        additions={diff.data?.additions ?? 0}
        deletions={diff.data?.deletions ?? 0}
      />
      <div className="min-h-0 flex-1 overflow-auto pb-5 font-mono text-[11.5px] leading-[1.8]">
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
                selectable
                selected={selected.has(lineKey(line))}
                contextActive={contextLine === lineKey(line)}
                onSelect={(shift) => toggleLine(i, shift)}
              />
            </DiffLineMenu>
          ) : (
            <DiffLineRow key={i} line={line} />
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
