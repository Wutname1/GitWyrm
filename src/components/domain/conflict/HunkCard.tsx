import { Check, GitMerge } from 'lucide-react'
import { cn } from '@/lib/utils'
import { Button } from '@/components/ui/button'
import { diffWords, type WordSpan } from '@/lib/wordDiff'
import { isDecided, type Choice, type ConflictSection, type LinePick } from '@/lib/conflictHunks'

/**
 * One contested region, with the choices that settle it.
 *
 * The whole point of the hunk view: show the few lines actually in dispute and
 * the controls that resolve them, instead of two whole files and an instruction
 * to find the difference yourself.
 *
 * Three levels of control, in the order they are usually reached for. Take one
 * side wholesale; keep both; or click individual lines from either version and
 * have them written in the order clicked. The last is what a conflict needs
 * when the answer is "my first line and their second" -- common enough that
 * having to drop into a text editor for it is the thing that makes conflict
 * resolution feel like a chore.
 */

const CODE = 'font-mono text-2xs leading-[1.7]'

/** Line rendering, optionally with the changed words within it picked out. */
function CodeText({ text, spans }: { text: string; spans?: WordSpan[] }) {
  // An empty line still needs height, or the row collapses and the picked-line
  // numbering stops matching what the user sees.
  if (text === '') return <>&nbsp;</>
  if (!spans || spans.length === 0) return <>{text}</>

  return (
    <>
      {spans.map((span, i) => (
        <span
          key={i}
          className={cn(span.changed && 'rounded-[2px] bg-foreground/15 text-foreground')}
        >
          {span.text}
        </span>
      ))}
    </>
  )
}

type SideName = 'ours' | 'theirs' | 'base'

/** One side's lines, each individually selectable. */
function Side({
  label,
  tone,
  lines,
  spans,
  whole,
  onTakeWhole,
  /** Order number shown on a picked line, or undefined when not picked. */
  pickOrder,
  onToggleLine,
  /** Line picking is off until the user opts into it, to keep the card calm. */
  picking,
}: {
  label: string
  tone: SideName
  lines: string[]
  spans?: Map<number, WordSpan[]>
  whole: boolean
  onTakeWhole: () => void
  pickOrder: (index: number) => number | undefined
  onToggleLine: (index: number) => void
  picking: boolean
}) {
  const toneText =
    tone === 'ours' ? 'text-added' : tone === 'theirs' ? 'text-modified' : 'text-muted-foreground'
  const toneBg =
    tone === 'ours'
      ? 'bg-added/[.06]'
      : tone === 'theirs'
        ? 'bg-modified/[.06]'
        : 'bg-muted-foreground/[.06]'
  const toneBorder =
    tone === 'ours'
      ? 'border-l-added'
      : tone === 'theirs'
        ? 'border-l-modified'
        : 'border-l-muted-foreground'
  const toneAccent =
    tone === 'ours'
      ? 'border-added bg-added text-background'
      : tone === 'theirs'
        ? 'border-modified bg-modified text-background'
        : 'border-muted-foreground bg-muted-foreground text-background'

  return (
    <div
      className={cn('min-w-0 flex-1 border-l-2', whole ? toneBorder : 'border-l-transparent')}
    >
      <div className="flex items-center gap-2 px-2.5 py-1">
        <span className={cn('text-2xs font-bold tracking-[.04em]', toneText)}>{label}</span>
        <Button
          size="sm"
          variant="ghost"
          onClick={(e) => {
            e.stopPropagation()
            onTakeWhole()
          }}
          aria-pressed={whole}
          className={cn(
            'ml-auto h-auto rounded-[3px] px-1.5 py-0.5 text-2xs font-semibold',
            whole ? 'bg-soft text-accent-text hover:bg-soft' : 'text-sub hover:text-foreground'
          )}
        >
          {whole && <Check size={10} strokeWidth={3} className="mr-0.5" />}
          Use all
        </Button>
      </div>

      <div className={cn('pb-1.5', toneBg, CODE)}>
        {lines.length === 0 ? (
          <div className="px-2.5 italic text-muted-foreground">(nothing on this side)</div>
        ) : (
          lines.map((line, i) => {
            const order = pickOrder(i)
            const picked = order !== undefined
            return (
              <button
                key={i}
                type="button"
                // A whole-side choice is the same decision expressed faster, so
                // clicking a line while one is active refines it rather than
                // fighting it -- the parent converts it to a line selection.
                onClick={(e) => {
                  e.stopPropagation()
                  onToggleLine(i)
                }}
                aria-pressed={picked}
                title={picking ? 'Click to include this line in the result' : undefined}
                className={cn(
                  'flex w-full items-start gap-1.5 px-2.5 text-left transition-colors',
                  picking && 'hover:bg-foreground/[.06]',
                  picked && 'bg-foreground/[.08]'
                )}
              >
                {picking && (
                  <span
                    className={cn(
                      'mt-[3px] flex h-3 w-3 flex-none items-center justify-center rounded-[3px] border text-[8px] font-bold leading-none',
                      picked ? toneAccent : 'border-border text-transparent'
                    )}
                  >
                    {order}
                  </span>
                )}
                <span
                  className={cn(
                    'min-w-0 flex-1 whitespace-pre',
                    picking && !picked && 'opacity-55'
                  )}
                >
                  <CodeText text={line} spans={spans?.get(i)} />
                </span>
              </button>
            )
          })
        )}
      </div>
    </div>
  )
}

export interface HunkCardProps {
  section: ConflictSection
  /** 1-based position, for "Conflict 2 of 5". */
  index: number
  total: number
  choice: Choice | undefined
  onChoose: (choice: Choice | undefined) => void
  /** Scrolled to and outlined when this is the one being stepped through. */
  focused: boolean
  onFocus: () => void
  showBase: boolean
  /** Whether per-line picking is switched on for the file. */
  picking: boolean
}

export function HunkCard({
  section,
  index,
  total,
  choice,
  onChoose,
  focused,
  onFocus,
  showBase,
  picking,
}: HunkCardProps) {
  const resolved = isDecided(choice)
  const picks: LinePick[] = typeof choice === 'object' ? choice.picks : []

  /** Where this line sits in the picked order, 1-based, if it is picked. */
  const pickOrder = (side: SideName) => (i: number) => {
    const at = picks.findIndex((p) => p.side === side && p.index === i)
    return at === -1 ? undefined : at + 1
  }

  /**
   * Add or remove one line from the selection.
   *
   * A whole-side choice becomes the equivalent line selection first, so that
   * clicking a line while "Use all" is active reads as narrowing that choice
   * rather than silently discarding it.
   */
  const toggleLine = (side: SideName) => (i: number) => {
    const current: LinePick[] =
      typeof choice === 'object'
        ? choice.picks
        : choice === 'ours' || choice === 'theirs' || choice === 'base'
          ? section[choice].map((_, n) => ({ side: choice, index: n }))
          : choice === 'both-ours-first'
            ? [
                ...section.ours.map((_, n) => ({ side: 'ours' as const, index: n })),
                ...section.theirs.map((_, n) => ({ side: 'theirs' as const, index: n })),
              ]
            : choice === 'both-theirs-first'
              ? [
                  ...section.theirs.map((_, n) => ({ side: 'theirs' as const, index: n })),
                  ...section.ours.map((_, n) => ({ side: 'ours' as const, index: n })),
                ]
              : []

    const at = current.findIndex((p) => p.side === side && p.index === i)
    const next =
      at === -1 ? [...current, { side, index: i }] : current.filter((_, n) => n !== at)

    // Clearing the last line returns the hunk to undecided rather than leaving
    // a selection that would silently delete the region.
    onChoose(next.length === 0 ? undefined : { kind: 'lines', picks: next })
  }

  /** Take one whole side, or clear it when it is already the choice. */
  const takeWhole = (side: SideName) => () =>
    onChoose(choice === side ? undefined : side)

  /**
   * Word-level highlighting, but only when the two sides line up one-to-one.
   *
   * Comparing line 1 against line 1 is meaningful for an edit to the same line
   * and misleading for a rewrite of a whole block, where it would mark almost
   * every word and say nothing. Line counts matching is a cheap, honest proxy
   * for "the same lines, edited".
   */
  const wordSpans =
    section.ours.length === section.theirs.length
      ? (() => {
          const ours = new Map<number, WordSpan[]>()
          const theirs = new Map<number, WordSpan[]>()
          section.ours.forEach((line, i) => {
            const pair = diffWords(line, section.theirs[i])
            if (!pair) return
            ours.set(i, pair.removed)
            theirs.set(i, pair.added)
          })
          return { ours, theirs }
        })()
      : null

  /** What the header says has been decided. */
  const summary =
    typeof choice === 'object'
      ? `Keeping ${choice.picks.length} line${choice.picks.length === 1 ? '' : 's'}`
      : choice === 'ours'
        ? 'Keeping yours'
        : choice === 'theirs'
          ? 'Keeping theirs'
          : choice === 'base'
            ? 'Keeping the original'
            : 'Keeping both'

  return (
    <div
      data-hunk={section.id}
      onClick={onFocus}
      className={cn(
        'overflow-hidden rounded border transition-colors',
        focused ? 'border-primary/60 ring-1 ring-primary/25' : 'border-border',
        resolved && !picking && 'opacity-75'
      )}
    >
      <div className="flex items-center gap-2 border-b border-border bg-panel2 px-2.5 py-1">
        <GitMerge size={11} className={cn('flex-none', resolved ? 'text-added' : 'text-removed')} />
        <span className="text-2xs font-semibold text-sub">
          Conflict {index} of {total}
        </span>

        {resolved ? (
          <span className="ml-auto flex items-center gap-1 text-2xs font-semibold text-added">
            <Check size={11} strokeWidth={3} />
            {summary}
            <Button
              size="sm"
              variant="ghost"
              onClick={(e) => {
                e.stopPropagation()
                onChoose(undefined)
              }}
              className="h-auto rounded-[3px] px-1.5 py-0.5 text-2xs font-semibold text-sub hover:text-foreground"
            >
              Undo
            </Button>
          </span>
        ) : (
          <div className="ml-auto flex items-center gap-1">
            <span className="mr-0.5 text-2xs text-muted-foreground">Keep both:</span>
            <Button
              size="sm"
              variant="ghost"
              onClick={(e) => {
                e.stopPropagation()
                onChoose('both-ours-first')
              }}
              className="h-auto rounded-[3px] px-1.5 py-0.5 text-2xs font-semibold text-sub hover:text-foreground"
              tooltip="Keep both versions, yours first"
            >
              Yours first
            </Button>
            <Button
              size="sm"
              variant="ghost"
              onClick={(e) => {
                e.stopPropagation()
                onChoose('both-theirs-first')
              }}
              className="h-auto rounded-[3px] px-1.5 py-0.5 text-2xs font-semibold text-sub hover:text-foreground"
              tooltip="Keep both versions, theirs first"
            >
              Theirs first
            </Button>
          </div>
        )}
      </div>

      <div className="flex flex-col divide-y divide-border sm:flex-row sm:divide-x sm:divide-y-0">
        <Side
          label="YOURS (current)"
          tone="ours"
          lines={section.ours}
          spans={wordSpans?.ours}
          whole={choice === 'ours'}
          onTakeWhole={takeWhole('ours')}
          pickOrder={pickOrder('ours')}
          onToggleLine={toggleLine('ours')}
          picking={picking}
        />
        <Side
          label="THEIRS (incoming)"
          tone="theirs"
          lines={section.theirs}
          spans={wordSpans?.theirs}
          whole={choice === 'theirs'}
          onTakeWhole={takeWhole('theirs')}
          pickOrder={pickOrder('theirs')}
          onToggleLine={toggleLine('theirs')}
          picking={picking}
        />
      </div>

      {/* The common ancestor explains *why* this is a conflict, but it is rarely
          what you want to keep, so it is opt-in and sits below the two sides. */}
      {showBase && section.base.length > 0 && (
        <div className="border-t border-border">
          <Side
            label="ORIGINAL (before both changes)"
            tone="base"
            lines={section.base}
            whole={choice === 'base'}
            onTakeWhole={takeWhole('base')}
            pickOrder={pickOrder('base')}
            onToggleLine={toggleLine('base')}
            picking={picking}
          />
        </div>
      )}

      {picking && (
        <div className="border-t border-border bg-panel2/50 px-2.5 py-1 text-2xs text-muted-foreground">
          {picks.length === 0
            ? 'Click any line to include it. They are kept in the order you click.'
            : `Result: ${picks.length} line${picks.length === 1 ? '' : 's'}, in the order numbered.`}
        </div>
      )}
    </div>
  )
}
