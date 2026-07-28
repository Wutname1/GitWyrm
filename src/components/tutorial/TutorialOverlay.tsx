import { useEffect, useMemo, useRef, useState } from 'react'
import { createPortal } from 'react-dom'
import { MousePointer2 } from 'lucide-react'
import { cn } from '@/lib/utils'
import type { GestureKind } from '@/lib/tutorialLessons'
import { useTutorialTargets, type TargetRect } from './useTutorialTarget'

/** Corner rounding on the knocked-out hole. */
const RADIUS = 8

interface TutorialOverlayProps {
  /** Ids to leave undimmed and clickable. The first anchors the coach card. */
  targetIds: string[]
  /**
   * Groups of `targetIds` to ring. Each group gets one ring around its
   * combined bounds. Defaults to one ring per target.
   */
  ringTargetIds?: string[][]
  /** Indexes into `targetIds` the ghost drag runs between. */
  dragFromIndex?: number
  dropTargetIndex?: number
  /** A second drag path mimed alongside the first. */
  secondaryDragFromIndex?: number
  secondaryDropTargetIndex?: number
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
  ringTargetIds,
  dragFromIndex = 0,
  dropTargetIndex,
  secondaryDragFromIndex,
  secondaryDropTargetIndex,
  gesture,
  successNonce,
  children,
}: TutorialOverlayProps) {
  const rects = useTutorialTargets(targetIds, true)
  // The coach card anchors to the first ringed target when there is one: with a
  // whole panel kept live, target 0 is that panel, and placing the card
  // relative to it would push it off beside the window rather than next to the
  // thing the user has to touch.
  const anchorIndex = ringTargetIds?.[0]?.[0]
    ? targetIds.indexOf(ringTargetIds[0][0])
    : 0
  const rect = rects[anchorIndex] ?? rects[0] ?? null
  const dragRect = rects[dragFromIndex] ?? rect
  const dropRect = dropTargetIndex == null ? null : (rects[dropTargetIndex] ?? null)
  const secondDragRect =
    secondaryDragFromIndex == null ? null : (rects[secondaryDragFromIndex] ?? null)
  const secondDropRect =
    secondaryDropTargetIndex == null ? null : (rects[secondaryDropTargetIndex] ?? null)

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

  // Rings point at the thing to touch; cutouts decide what stays usable. They
  // differ whenever a lesson keeps a whole panel live -- ringing that panel
  // would draw a pulsing box around half the window instead of an instruction.
  const ringRects = useMemo(() => {
    if (!ringTargetIds?.length) return cutouts
    // One ring per group, drawn around everything in it. Groups whose members
    // have not rendered yet contribute nothing rather than a zero-size ring.
    return ringTargetIds
      .map((group) => {
        const members = group
          .map((id) => rects[targetIds.indexOf(id)] ?? null)
          .filter((r): r is TargetRect => r !== null)
        if (members.length === 0) return null
        const top = Math.min(...members.map((m) => m.top))
        const left = Math.min(...members.map((m) => m.left))
        const right = Math.max(...members.map((m) => m.left + m.width))
        const bottom = Math.max(...members.map((m) => m.top + m.height))
        return { top, left, width: right - left, height: bottom - top }
      })
      .filter((r): r is TargetRect => r !== null)
  }, [ringTargetIds, targetIds, rects, cutouts])

  /**
   * Bands walling off everything that is not a lit target.
   *
   * Computed by slicing the full window against each cutout in turn, rather
   * than taking one bounding box around all of them. The bounding-box version
   * looked right for two adjacent sidebar rows but opened a corridor spanning
   * everything between them the moment the targets were far apart -- lesson
   * one lights a sidebar row and a graph pill, and the band between the two
   * left commit rows clickable, so a double-click could land on a commit and
   * the lesson would never see its branch switch.
   *
   * Slicing keeps exactly the targets open and nothing else, and because the
   * cutouts already carry padding, adjacent targets still leave a usable
   * corridor between them for a drag to cross.
   *
   * With nothing measured yet the whole window is blocked, which is the safe
   * side to fail on -- the app stays inert rather than half-usable under a
   * scrim the user cannot see through.
   */
  const blockers = useMemo(() => {
    const { width, height } = viewport
    let bands = [{ top: 0, left: 0, width, height }]

    for (const hole of cutouts) {
      const next = []
      for (const b of bands) {
        const bRight = b.left + b.width
        const bBottom = b.top + b.height
        const hRight = hole.left + hole.width
        const hBottom = hole.top + hole.height

        // No overlap: this band is unaffected by the hole.
        if (hole.left >= bRight || hRight <= b.left || hole.top >= bBottom || hBottom <= b.top) {
          next.push(b)
          continue
        }

        // Split the band into up to four pieces around the hole.
        if (hole.top > b.top) {
          next.push({ top: b.top, left: b.left, width: b.width, height: hole.top - b.top })
        }
        if (hBottom < bBottom) {
          next.push({ top: hBottom, left: b.left, width: b.width, height: bBottom - hBottom })
        }
        const midTop = Math.max(b.top, hole.top)
        const midBottom = Math.min(bBottom, hBottom)
        const midHeight = midBottom - midTop
        if (midHeight > 0) {
          if (hole.left > b.left) {
            next.push({ top: midTop, left: b.left, width: hole.left - b.left, height: midHeight })
          }
          if (hRight < bRight) {
            next.push({ top: midTop, left: hRight, width: bRight - hRight, height: midHeight })
          }
        }
      }
      bands = next
    }

    return bands
  }, [cutouts, viewport])

  const overlay = (
    <div className="pointer-events-none fixed inset-0 z-[200]">
      {/* The dimming, drawn as one masked shape so the hole has soft corners
          and no seams. Painting only -- `pointer-events-none` is essential: an
          SVG mask controls what is drawn, never what is clickable, so leaving
          this interactive made the scrim swallow every click across the whole
          window and the "hole" was purely cosmetic. The blockers below are what
          actually gate input. */}
      <svg
        width={viewport.width}
        height={viewport.height}
        className="pointer-events-none absolute inset-0"
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

      {/* The actual input gate: four bands around the lit area, leaving the
          target itself uncovered so the real element receives the gesture. */}
      {blockers.map((b, i) => (
        <div
          key={`block-${i}`}
          className="pointer-events-auto absolute"
          style={{ top: b.top, left: b.left, width: b.width, height: b.height }}
          aria-hidden
        />
      ))}

      {/* Rings around the thing to touch. Never interactive -- they sit
          directly over the element the user has to click. */}
      {ringRects.map((c, i) => (
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

      {dragRect && (
        <GhostGesture gesture={gesture} from={dragRect} to={dropRect ?? undefined} />
      )}
      {secondDragRect && secondDropRect && (
        <GhostGesture gesture={gesture} from={secondDragRect} to={secondDropRect} />
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
