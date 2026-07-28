import { useEffect, useMemo, useState } from 'react'
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

/**
 * Nudge the card away from anything the lesson just opened on top of the app.
 *
 * Context menus and dialogs render into portals at the end of `document.body`,
 * so they are findable without the lesson knowing they exist. Whichever side
 * has more free space wins; if neither side fits, the card drops to the bottom
 * of the window instead, which is always clear of a centred dialog and of a
 * menu that opens upward from a click.
 *
 * Without this, the card sat over the very menu or panel the lesson had just
 * told the user to read -- true even docked, because on a narrow window the
 * card is wider than the sidebar it is docked against.
 */
function useAvoidOverlays(
  preferred: { top: number; left: number },
  height: number
): { top: number; left: number } {
  const [position, setPosition] = useState(preferred)

  useEffect(() => {
    let frame = 0

    const settle = () => {
      const overlays = Array.from(
        document.querySelectorAll<HTMLElement>(
          '[data-radix-menu-content], [role="dialog"]:not([data-tutorial-card])'
        )
      )
        .map((node) => node.getBoundingClientRect())
        .filter((box) => box.width > 0 && box.height > 0)

      if (overlays.length === 0) {
        setPosition(preferred)
        return
      }

      const card = {
        left: preferred.left,
        right: preferred.left + CARD_WIDTH,
        top: preferred.top,
        bottom: preferred.top + height,
      }
      const hits = overlays.filter(
        (o) =>
          o.left < card.right && o.right > card.left && o.top < card.bottom && o.bottom > card.top
      )
      if (hits.length === 0) {
        setPosition(preferred)
        return
      }

      // Slide horizontally to whichever side of the obstruction has room.
      const leftMost = Math.min(...hits.map((h) => h.left))
      const rightMost = Math.max(...hits.map((h) => h.right))
      const roomLeft = leftMost - MARGIN
      const roomRight = window.innerWidth - rightMost - MARGIN

      // Prefer sliding sideways: it keeps the card at full height, next to
      // whatever the user is reading, rather than banished to an edge.
      if (roomLeft >= CARD_WIDTH && roomLeft >= roomRight) {
        setPosition({ top: preferred.top, left: leftMost - CARD_WIDTH - GAP })
        return
      }
      if (roomRight >= CARD_WIDTH) {
        setPosition({ top: preferred.top, left: rightMost + GAP })
        return
      }

      // Nothing fits beside it: go above or below, whichever has room. A menu
      // tall enough to leave neither (rare, but a long commit menu on a short
      // window gets close) falls back to whichever gap is larger, so the card
      // is at worst partly covered rather than buried in the middle of it.
      const topMost = Math.min(...hits.map((h) => h.top))
      const bottomMost = Math.max(...hits.map((h) => h.bottom))
      const roomAbove = topMost - MARGIN
      const roomBelow = window.innerHeight - bottomMost - MARGIN

      if (roomBelow >= height) {
        setPosition({ top: bottomMost + GAP, left: preferred.left })
      } else if (roomAbove >= height) {
        setPosition({ top: topMost - height - GAP, left: preferred.left })
      } else if (roomBelow >= roomAbove) {
        setPosition({
          top: Math.max(MARGIN, window.innerHeight - height - MARGIN),
          left: preferred.left,
        })
      } else {
        setPosition({ top: MARGIN, left: preferred.left })
      }
    }

    // Menus mount a frame or two after the click that opened them, so this
    // watches the DOM rather than measuring once.
    const schedule = () => {
      if (frame) return
      frame = requestAnimationFrame(() => {
        frame = 0
        settle()
      })
    }

    settle()
    const observer = new MutationObserver(schedule)
    observer.observe(document.body, { childList: true, subtree: true })
    window.addEventListener('resize', schedule)
    return () => {
      if (frame) cancelAnimationFrame(frame)
      observer.disconnect()
      window.removeEventListener('resize', schedule)
    }
  }, [preferred, height])

  return position
}

interface TutorialCoachProps {
  lesson: Lesson
  rect: TargetRect | null
  /**
   * Dock the card against a side panel instead of placing it near the target.
   *
   * For lessons whose gesture spans several places, or whose result appears in
   * the middle of the screen -- a context menu, a modal -- there is no good
   * spot near the target. Centring it put the card straight over the thing the
   * lesson had just told the user to open. Docking to a side keeps it clear of
   * the graph, which is where all of that happens.
   */
  dock?: 'left' | 'right'
  step: number
  total: number
  /** True once the gesture landed, so the card can show the success line. */
  succeeded: boolean
  onSkip: () => void
  /**
   * Advance from the success line. Separate from `onSkip` because a lesson
   * with a follow-up has somewhere to go next that is not the next lesson --
   * wiring this button to skip would jump straight past the panel the user
   * just opened, which is the part worth reading.
   */
  onAdvance: () => void
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
  dock,
  step,
  total,
  succeeded,
  onSkip,
  onAdvance,
  onExit,
}: TutorialCoachProps) {
  // Estimated rather than measured: measuring would need a layout pass after
  // paint, which makes the card visibly jump into place on every step.
  const height = 210

  // Where the card would like to sit.
  const preferred = useMemo(() => {
    if (dock === 'left') {
      return { top: window.innerHeight / 2 - height / 2, left: MARGIN }
    }
    if (dock === 'right') {
      return {
        top: window.innerHeight / 2 - height / 2,
        left: window.innerWidth - CARD_WIDTH - MARGIN,
      }
    }
    return placeCard(rect, height)
  }, [rect, dock])

  // ...and where it actually ends up, once anything the lesson just opened is
  // taken into account. A dock alone is not enough: on a narrow window the card
  // is wider than the sidebar, so even the far edge still overlaps the graph
  // where context menus appear. This measures the live menu/dialog and slides
  // the card clear of it.
  const position = useAvoidOverlays(preferred, height)

  return (
    <div
      className="pointer-events-auto absolute w-80 rounded-xl border border-border bg-panel p-4 shadow-2xl"
      style={{ top: position.top, left: position.left, width: CARD_WIDTH }}
      role="dialog"
      // Excluded from the overlay scan below, which also matches role=dialog:
      // without this the card measures itself and tries to dodge its own box.
      data-tutorial-card
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
          {/* Once the gesture lands, the step advances on a timer -- but the
              success line is where the lesson explains what just happened, so
              there is a button to move on immediately rather than only the
              option to wait. Reading speed is the user's, not ours. */}
          {succeeded ? (
            <Button size="sm" onClick={onAdvance} className="h-7 text-2xs">
              {step + 1 === total ? 'Finish' : 'Next'}
            </Button>
          ) : (
            <Button variant="ghost" size="sm" onClick={onSkip} className="h-7 text-2xs text-sub">
              Skip this step
            </Button>
          )}
        </div>
      </div>
    </div>
  )
}
