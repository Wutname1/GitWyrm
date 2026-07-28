import { useMemo } from 'react'
import { Hand, X } from 'lucide-react'
import { cn } from '@/lib/utils'
import { Button } from '@/components/ui/button'
import type { Lesson } from '@/lib/tutorialLessons'
import type { TargetRect } from './useTutorialTarget'

const CARD_WIDTH = 320
/** Gap between the spotlight and the card. */
const GAP = 16
/** Keep the card off the window edges. */
const MARGIN = 12

/**
 * Place the card beside the spotlight, on whichever side has room.
 *
 * Preference order is below, above, right, then left: below reads most
 * naturally for a left-to-right, top-to-bottom scan, and the sidebar targets
 * this tutorial uses are tall and narrow, so a card beside them would crowd the
 * hole. Falls back to centred when nothing is measured yet.
 */
function placeCard(rect: TargetRect | null, height: number) {
  if (!rect) {
    return {
      top: window.innerHeight / 2 - height / 2,
      left: window.innerWidth / 2 - CARD_WIDTH / 2,
    }
  }

  const clampLeft = (value: number) =>
    Math.max(MARGIN, Math.min(value, window.innerWidth - CARD_WIDTH - MARGIN))
  const clampTop = (value: number) =>
    Math.max(MARGIN, Math.min(value, window.innerHeight - height - MARGIN))

  const below = rect.top + rect.height + GAP
  if (below + height + MARGIN <= window.innerHeight) {
    return { top: below, left: clampLeft(rect.left) }
  }

  const above = rect.top - GAP - height
  if (above >= MARGIN) {
    return { top: above, left: clampLeft(rect.left) }
  }

  const right = rect.left + rect.width + GAP
  if (right + CARD_WIDTH + MARGIN <= window.innerWidth) {
    return { top: clampTop(rect.top), left: right }
  }

  return { top: clampTop(rect.top), left: clampLeft(rect.left - GAP - CARD_WIDTH) }
}

interface TutorialCoachProps {
  lesson: Lesson
  rect: TargetRect | null
  step: number
  total: number
  /** True once the gesture landed, so the card can show the success line. */
  succeeded: boolean
  onSkip: () => void
  onExit: () => void
}

/**
 * The instruction card that rides alongside the spotlight.
 *
 * Deliberately never carries a "Next" button for the gesture itself: the step
 * advances because the user did the thing, which is the entire point of a
 * hands-on tour. "Skip this step" is the escape hatch, and it is always visible
 * so nobody can get stuck on a gesture their hardware or hand makes awkward.
 */
export function TutorialCoach({
  lesson,
  rect,
  step,
  total,
  succeeded,
  onSkip,
  onExit,
}: TutorialCoachProps) {
  // Estimated rather than measured: measuring would need a layout pass after
  // paint, which makes the card visibly jump into place on every step.
  const height = 210
  const position = useMemo(() => placeCard(rect, height), [rect])

  return (
    <div
      className="pointer-events-auto absolute w-80 rounded-xl border border-border bg-panel p-4 shadow-2xl"
      style={{ top: position.top, left: position.left, width: CARD_WIDTH }}
      role="dialog"
      aria-live="polite"
      aria-label={lesson.title}
    >
      <div className="flex items-start justify-between gap-2">
        <span className="font-mono text-2xs uppercase tracking-wide text-muted-foreground">
          Step {step + 1} of {total}
        </span>
        <button
          onClick={onExit}
          aria-label="Exit tutorial"
          className="-mr-1 -mt-1 rounded p-1 text-muted-foreground hover:bg-panel3 hover:text-foreground"
        >
          <X size={13} />
        </button>
      </div>

      <h2 className="mt-2 text-sm font-semibold text-foreground">{lesson.title}</h2>
      <p className="mt-1.5 text-[0.78125rem] leading-relaxed text-sub">{lesson.body}</p>

      <div
        className={cn(
          'mt-3 flex items-start gap-2 rounded-lg border px-2.5 py-2',
          succeeded
            ? 'border-[var(--gw-green)]/40 bg-[var(--gw-green)]/10'
            : 'border-border bg-background'
        )}
      >
        <Hand
          size={13}
          className={cn(
            'mt-px flex-none',
            succeeded ? 'text-[var(--gw-green)]' : 'text-accent-text'
          )}
        />
        <span className="text-2xs leading-relaxed text-foreground">
          {succeeded ? lesson.success : lesson.instruction}
        </span>
      </div>

      <div className="mt-3 flex items-center justify-between">
        <div className="flex gap-1" aria-hidden>
          {Array.from({ length: total }, (_, i) => (
            <span
              key={i}
              className={cn(
                'h-1.5 rounded-full transition-all',
                i === step ? 'w-4 bg-primary' : 'w-1.5 bg-border'
              )}
            />
          ))}
        </div>
        <div className="flex gap-2">
          <Button variant="ghost" size="sm" onClick={onSkip} className="h-7 text-2xs text-sub">
            Skip this step
          </Button>
        </div>
      </div>
    </div>
  )
}
