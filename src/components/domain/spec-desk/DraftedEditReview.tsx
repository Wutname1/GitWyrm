import { useMemo } from 'react'
import { Check, X } from 'lucide-react'
import { cn } from '@/lib/utils'
import { diffLines, countChanges, isUnchanged } from '@/lib/textDiff'
import { computeWordSpans } from '@/lib/wordDiff'

/**
 * A drafted edit, shown as a difference against the file on disk.
 *
 * The point of showing a diff rather than the proposed result is that a reader
 * can check what the AI touched. A file that comes back looking plausible but
 * quietly dropped a requirement is exactly the failure this is here to catch, and
 * it is invisible if you only see the new version.
 */
export function DraftedEditReview({
  file,
  summary,
  current,
  proposed,
  onAccept,
  onReject,
}: {
  file: string
  /** The AI's one-line description of what it changed. */
  summary: string
  /** What is on disk right now. */
  current: string
  /** What the AI proposes the file should become. */
  proposed: string
  /** Opens the draft in the editor as unsaved text. Never writes. */
  onAccept: () => void
  onReject: () => void
}) {
  const lines = useMemo(() => diffLines(current, proposed), [current, proposed])
  const spans = useMemo(
    () => computeWordSpans(lines.map((l) => ({ sign: l.sign, text: l.text }))),
    [lines]
  )
  const counts = useMemo(() => countChanges(lines), [lines])
  const unchanged = isUnchanged(lines)

  return (
    <div className="overflow-hidden rounded-lg border border-border">
      <header className="border-b border-border bg-panel px-3 py-2">
        <div className="flex items-center justify-between gap-3">
          <span className="min-w-0 overflow-hidden text-ellipsis whitespace-nowrap font-mono text-2xs text-sub">
            {file}
          </span>
          <span className="flex-none font-mono text-2xs text-muted-foreground">
            {unchanged ? (
              'no change'
            ) : (
              <>
                <span className="text-added">+{counts.added}</span>{' '}
                <span className="text-removed">-{counts.removed}</span>
              </>
            )}
          </span>
        </div>
        <p className="mt-1 text-2xs leading-snug text-foreground">{summary}</p>
      </header>

      {unchanged ? (
        <p className="px-3 py-3 text-2xs text-muted-foreground">
          The AI sent back this file unchanged. Nothing to apply.
        </p>
      ) : (
        <div className="max-h-[min(50vh,420px)] overflow-auto">
          <table className="w-full border-collapse font-mono text-2xs">
            <tbody>
              {lines.map((line, i) => {
                const lineSpans = spans.get(i)
                return (
                  <tr
                    key={i}
                    className={cn(
                      line.sign === '+' && 'bg-added/10',
                      line.sign === '-' && 'bg-removed/10'
                    )}
                  >
                    <td
                      className={cn(
                        'w-5 select-none px-1.5 text-right align-top',
                        line.sign === '+' && 'text-added',
                        line.sign === '-' && 'text-removed',
                        line.sign === ' ' && 'text-muted-foreground'
                      )}
                    >
                      {line.sign === ' ' ? '' : line.sign}
                    </td>
                    <td className="whitespace-pre-wrap break-words py-px pr-2 align-top text-foreground">
                      {lineSpans
                        ? lineSpans.map((span, s) => (
                            <span
                              key={s}
                              className={cn(
                                span.changed &&
                                  (line.sign === '+'
                                    ? 'rounded-sm bg-added/30'
                                    : 'rounded-sm bg-removed/30')
                              )}
                            >
                              {span.text}
                            </span>
                          ))
                        : line.text}
                      {/* An empty line still needs height, or the diff collapses. */}
                      {line.text === '' && ' '}
                    </td>
                  </tr>
                )
              })}
            </tbody>
          </table>
        </div>
      )}

      <footer className="flex items-center justify-between gap-3 border-t border-border bg-panel px-3 py-2">
        <p className="text-2xs text-muted-foreground">
          Nothing is saved yet. Accepting opens this in the editor.
        </p>
        <div className="flex flex-none items-center gap-1.5">
          <button
            onClick={onReject}
            className="inline-flex items-center gap-1 rounded px-2 py-1 text-2xs font-semibold text-sub transition-colors hover:text-foreground"
          >
            <X size={11} />
            Reject
          </button>
          <button
            onClick={onAccept}
            disabled={unchanged}
            className="inline-flex items-center gap-1 rounded bg-primary px-2 py-1 text-2xs font-semibold text-accent-fg transition-opacity hover:opacity-90 disabled:opacity-40"
          >
            <Check size={11} />
            Open in the editor
          </button>
        </div>
      </footer>
    </div>
  )
}
