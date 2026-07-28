import { useEffect, useMemo, useRef, useState } from 'react'
import { createPortal } from 'react-dom'
import { MousePointer2 } from 'lucide-react'
import { cn } from '@/lib/utils'
import type { GestureKind } from '@/lib/tutorialLessons'
import { useTutorialTargets, type TargetRect } from './useTutorialTarget'

/** Corner rounding on the knocked-out hole. */
const RADIUS = 8

interface TutorialOverlayProps {
  /** Ids to leave lit. The first anchors the coach card and the ghost. */
  targetIds: string[]
  gesture: GestureKind
  /** Bumped when the gesture lands, to play the success flare. */
  successNonce: number
  children: (rect: TargetRect | null) => React.ReactNode
}

/**
 * Dims the whole window except a hole around the lesson's target.
 *
 * The dimming is one full-screen SVG with the cutout punched out of its fill
 * rule, rather than four rectangles around the target: a single shape means the
 * hole can have rounded corners and the edge never shows a seam when the target
 * moves.
 *
 * Clicks are the fiddly part. The scrim swallows every pointer event so the
 * dimmed UI genuinely cannot be used -- that is what makes the tutorial guided
 * rather than a suggestion -- while the hole is left open so the real element
 * underneath still receives the gesture. The user is therefore practising on
 * the actual UI, not a mock of it.
 */
export function TutorialOverlay({
  targetIds,
  gesture,
  successNonce,
  children,
}: TutorialOverlayProps) {
  const rects = useTutorialTargets(targetIds, true)
  const rect = rects[0] ?? null
  const dropRect = rects[1] ?? null

  // Replay the flare whenever a gesture lands. Keyed off the nonce so a repeat
  // of the same lesson still animates.
  const [flare, setFlare] = useState(0)
  const seen = useRef(successNonce)
  useEffect(() => {
    if (successNonce !== seen.current) {
      seen.current = successNonce
      setFlare((n) => n + 1)
    }
  }, [successNonce])

  const [viewport, setViewport] = useState(() => ({
    width: window.innerWidth,
    height: window.innerHeight,
  }))
  useEffect(() => {
    const onResize = () =>
      setViewport({ width: window.innerWidth, height: window.innerHeight })
    window.addEventListener('resize', onResize)
    return () => window.removeEventListener('resize', onResize)
  }, [])

  // Both holes share one path so the drag lesson can light up its source and
  // its destination at once -- the gesture involves both, so dimming the
  // destination would hide half the instruction.
  const cutouts = useMemo(
    () => rects.filter((r): r is TargetRect => r !== null),
    [rects]
  )

  const overlay = (
    <div className="pointer-events-none fixed inset-0 z-[200]">
      {/* The scrim. pointer-events-auto so it eats clicks aimed at the dimmed
          UI; the holes below re-open only the target. */}
      <svg
        width={viewport.width}
        height={viewport.height}
        className="pointer-events-auto absolute inset-0"
        aria-hidden
      >
        <defs>
          <mask id="wyrm-tutorial-mask">
            <rect x={0} y={0} width={viewport.width} height={viewport.height} fill="white" />
            {cutouts.map((c, i) => (
              <rect
                key={i}
                x={c.left}
                y={c.top}
                width={c.width}
                height={c.height}
                rx={RADIUS}
                ry={RADIUS}
                fill="black"
              />
            ))}
          </mask>
        </defs>
        <rect
          x={0}
          y={0}
          width={viewport.width}
          height={viewport.height}
          fill="rgb(4 6 10 / 0.72)"
          mask="url(#wyrm-tutorial-mask)"
        />
      </svg>

      {/* Transparent windows over each hole. These sit above the scrim and let
          pointer events fall through to the real element beneath. */}
      {cutouts.map((c, i) => (
        <div
          key={i}
          className="pointer-events-none absolute"
          style={{ top: c.top, left: c.left, width: c.width, height: c.height }}
        >
          <div
            className={cn(
              'absolute inset-0 rounded-lg',
              flare > 0 && i === 0 ? 'wyrm-tutorial-hit' : 'wyrm-tutorial-ring'
            )}
            key={`${flare}-${i}`}
          />
        </div>
      ))}

      {rect && (
        <GhostGesture gesture={gesture} from={rect} to={dropRect ?? undefined} />
      )}

      {children(rect)}
    </div>
  )

  return createPortal(overlay, document.body)
}

/**
 * A looping mime of the gesture the lesson is asking for.
 *
 * Showing the motion is the difference between "drag one branch onto another"
 * as a sentence and as something the user has already watched happen. It is
 * decoration only -- it never blocks input, and reduced-motion hides it
 * entirely, since a still frame of a drag communicates nothing.
 */
function GhostGesture({
  gesture,
  from,
  to,
}: {
  gesture: GestureKind
  from: TargetRect
  to?: TargetRect
}) {
  // Start the pointer just inside the target rather than at its corner, so the
  // glyph reads as touching the element instead of floating beside it.
  const startX = from.left + Math.min(from.width * 0.5, 60)
  const startY = from.top + from.height * 0.6

  if (gesture === 'drag' && to) {
    const travelX = to.left + Math.min(to.width * 0.5, 60) - startX
    const travelY = to.top + to.height * 0.6 - startY
    return (
      <div
        className="wyrm-tutorial-travel pointer-events-none absolute"
        style={
          {
            top: startY,
            left: startX,
            '--gw-travel-x': `${travelX}px`,
            '--gw-travel-y': `${travelY}px`,
          } as React.CSSProperties
        }
        aria-hidden
      >
        <MousePointer2
          size={18}
          className="fill-white/90 text-slate-900 drop-shadow-[0_2px_6px_rgba(0,0,0,0.7)]"
        />
        {/* The chip being towed, so the motion reads as carrying something. */}
        <span className="absolute left-3 top-3 rounded-[5px] bg-primary px-1.5 py-px font-mono text-2xs font-semibold text-primary-foreground opacity-90 shadow-lg">
          branch
        </span>
      </div>
    )
  }

  return (
    <div
      className="wyrm-tutorial-tap pointer-events-none absolute"
      style={{ top: startY, left: startX }}
      aria-hidden
    >
      <MousePointer2
        size={18}
        className="fill-white/90 text-slate-900 drop-shadow-[0_2px_6px_rgba(0,0,0,0.7)]"
      />
    </div>
  )
}
