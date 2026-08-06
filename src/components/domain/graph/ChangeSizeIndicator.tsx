import { cn } from '@/lib/utils'

interface ChangeSizeIndicatorProps {
  /** `null` while the stats for this commit are still being computed. */
  filesChanged: number | null
  additions: number | null
  deletions: number | null
  showLineCounts: boolean
  mode: 'row' | 'column'
  className?: string
}

/** A compact logarithmic bar: length shows size; color shows added vs removed. */
export function ChangeSizeIndicator({
  filesChanged,
  additions,
  deletions,
  showLineCounts,
  mode,
  className,
}: ChangeSizeIndicatorProps) {
  // Stats are fetched for rows in view, so a commit can render before its
  // numbers exist. Hold the row's exact shape with a muted placeholder rather
  // than showing a misleading "0 files" or letting the layout jump when the
  // real numbers land a beat later.
  if (filesChanged == null || additions == null || deletions == null) {
    return (
      <div
        aria-label="Loading change size"
        className={cn(
          'flex min-w-0 animate-pulse items-center gap-1.5 font-mono text-2xs text-muted-foreground/40',
          className,
        )}
      >
        <span aria-hidden className="flex-none">
          &middot;&middot;&middot;
        </span>
        <span
          aria-hidden
          className="flex h-1.5 flex-none overflow-hidden rounded-sm bg-panel3"
          style={{ width: 28 }}
        />
      </div>
    )
  }

  const totalLines = additions + deletions
  const magnitude = totalLines > 0 ? totalLines : filesChanged
  const barWidth = magnitude > 0 ? Math.min(74, Math.max(10, 8 + Math.log2(magnitude + 1) * 8)) : 0
  const addedPercent = totalLines > 0 ? (additions / totalLines) * 100 : 0
  const removedPercent = totalLines > 0 ? (deletions / totalLines) * 100 : 0
  const label = `${filesChanged} changed ${filesChanged === 1 ? 'file' : 'files'}, ${additions} ${additions === 1 ? 'line' : 'lines'} added, ${deletions} ${deletions === 1 ? 'line' : 'lines'} removed`

  return (
    <div
      aria-label={label}
      className={cn(
        'flex min-w-0 items-center gap-1.5 font-mono text-2xs text-muted-foreground',
        className,
      )}
    >
      <span className="flex flex-none items-center gap-1 whitespace-nowrap">
        {filesChanged}{mode === 'row' && ` ${filesChanged === 1 ? 'file' : 'files'}`}
      </span>
      <span
        aria-hidden
        className="flex h-1.5 flex-none overflow-hidden rounded-sm bg-panel3"
        style={{ width: barWidth }}
      >
        {totalLines > 0 ? (
          <>
            <span className="h-full bg-added/60" style={{ width: `${addedPercent}%` }} />
            <span className="h-full bg-removed/60" style={{ width: `${removedPercent}%` }} />
          </>
        ) : filesChanged > 0 ? (
          <span className="h-full w-full bg-muted-foreground/60" />
        ) : null}
      </span>
      {showLineCounts && (
        <span className="flex min-w-0 items-center gap-1 whitespace-nowrap">
          <span className="text-added">+{additions}</span>
          <span className="text-removed">-{deletions}</span>
        </span>
      )}
    </div>
  )
}
