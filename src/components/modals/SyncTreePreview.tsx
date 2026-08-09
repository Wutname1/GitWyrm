import {
  GRAPH,
  armTop,
  dotCount,
  stackedLane,
  type Divergence,
  type PreviewMode,
} from '@/lib/syncPreview'

interface SyncTreePreviewProps {
  /** How the two refs stand against each other. */
  divergence: Divergence
  /** The outcome being drawn. */
  mode: PreviewMode
  /** Name of our side, for the arm label. */
  ourLabel?: string
  /** Name of their side, for the arm label. */
  theirLabel?: string
}

const OURS = 'var(--gw-lane0, #2dd4a7)'
const THEIRS = 'var(--gw-lane1, #38bdf8)'
const BASE = 'var(--gw-muted, #717171)'
const NEW = 'var(--gw-amber, #fbbf24)'
const LOST = 'var(--gw-red, #f87171)'
const SUB = 'var(--gw-sub, #9e9e9e)'

/**
 * A schematic of what the chosen option does to history: the current shape on
 * the left, the result on the right.
 *
 * Deliberately NOT a commit list. It draws at most three dots per arm and puts
 * the real count in the label, so the picture is identical whether you are two
 * commits diverged or forty -- the shape is the message, not the contents. That
 * is also what keeps it readable inside a 452px dialog.
 */
export function SyncTreePreview({
  divergence,
  mode,
  ourLabel = 'yours',
  theirLabel = 'theirs',
}: SyncTreePreviewProps) {
  const { ours, theirs } = divergence
  const danger = mode === 'replace' || mode === 'reset'

  return (
    <svg viewBox="0 0 330 186" preserveAspectRatio="xMidYMid meet" className="block h-[186px] w-full">
      <defs>
        <marker id="syncArrow" markerWidth="7" markerHeight="7" refX="5" refY="3" orient="auto">
          <path d="M0 0 L6 3 L0 6 z" fill="var(--gw-accent-text, #38b78e)" />
        </marker>
      </defs>

      <text x="10" y="13" fill={BASE} fontSize="9" letterSpacing=".8">
        NOW
      </text>
      <text x="196" y="13" fill={danger ? LOST : 'var(--gw-accent-text, #38b78e)'} fontSize="9" letterSpacing=".8">
        AFTER
      </text>
      <line x1="168" y1="22" x2="168" y2="178" stroke="var(--gw-border, #2d2d2d)" strokeDasharray="3 4" />

      <Before ours={ours} theirs={theirs} ourLabel={ourLabel} theirLabel={theirLabel} />

      <path
        d="M132 100 L158 100"
        stroke="var(--gw-accent-text, #38b78e)"
        strokeWidth="1.5"
        fill="none"
        strokeDasharray="4 5"
        markerEnd="url(#syncArrow)"
        className="motion-safe:animate-[syncDrift_1.1s_linear_infinite]"
      />

      <After mode={mode} ours={ours} theirs={theirs} ourLabel={ourLabel} theirLabel={theirLabel} />
    </svg>
  )
}

/** Dots for one arm, newest at the top. */
function Arm({ n, cx, color }: { n: number; cx: number; color: string }) {
  return (
    <>
      {Array.from({ length: dotCount(n) }, (_, i) => (
        <circle key={i} cx={cx} cy={GRAPH.baseDot - i * GRAPH.gap} r="4.2" fill={color} />
      ))}
    </>
  )
}

const count = (label: string, n: number) => `${label} ×${n}`

/** The current shape: either two arms off a shared base, or a single lane. */
function Before({
  ours,
  theirs,
  ourLabel,
  theirLabel,
}: {
  ours: number
  theirs: number
  ourLabel: string
  theirLabel: string
}) {
  const root = (
    <>
      <circle cx="62" cy={GRAPH.root} r="4.2" fill={BASE} />
      <text x="73" y={GRAPH.root + 3} fill={SUB} fontSize="8.5">
        shared base
      </text>
    </>
  )

  if (ours > 0 && theirs > 0) {
    return (
      <>
        <path
          d={`M62 ${GRAPH.root} L62 146 C62 134 36 134 36 122 L36 ${armTop(ours) - 14}`}
          stroke={OURS}
          strokeWidth="1.7"
          fill="none"
          opacity=".8"
        />
        <path
          d={`M62 ${GRAPH.root} L62 146 C62 134 96 134 96 122 L96 ${armTop(theirs) - 14}`}
          stroke={THEIRS}
          strokeWidth="1.7"
          fill="none"
          opacity=".8"
        />
        <Arm n={ours} cx={36} color={OURS} />
        <Arm n={theirs} cx={96} color={THEIRS} />
        {root}
        <text x="8" y={armTop(ours) - 24} fill={OURS} fontSize="8.5">
          {count(ourLabel, ours)}
        </text>
        <text x="106" y={armTop(theirs) - 24} fill={THEIRS} fontSize="8.5">
          {count(theirLabel, theirs)}
        </text>
      </>
    )
  }

  const n = ours || theirs
  const color = ours ? OURS : THEIRS
  const label = ours ? ourLabel : theirLabel
  return (
    <>
      <path
        d={`M62 ${GRAPH.root} L62 ${armTop(n) - 14}`}
        stroke={color}
        strokeWidth="1.7"
        fill="none"
        opacity=".8"
      />
      <Arm n={n} cx={62} color={color} />
      {root}
      <text x="74" y={armTop(n) - 10} fill={color} fontSize="8.5">
        {count(label, n)}
      </text>
    </>
  )
}

/** The resulting shape, which is what the whole component exists to show. */
function After({
  mode,
  ours,
  theirs,
  ourLabel,
  theirLabel,
}: {
  mode: PreviewMode
  ours: number
  theirs: number
  ourLabel: string
  theirLabel: string
}) {
  // Force-push: our lane survives intact, theirs floats away struck out.
  if (mode === 'replace' || mode === 'reset') {
    const keptN = mode === 'replace' ? ours : theirs
    const keptColor = mode === 'replace' ? OURS : THEIRS
    const keptLabel = mode === 'replace' ? ourLabel : theirLabel
    const lostN = mode === 'replace' ? theirs : ours
    const lostLabel = mode === 'replace' ? theirLabel : ourLabel
    return (
      <>
        <path
          d={`M236 ${GRAPH.root} L236 ${armTop(keptN) - 14}`}
          stroke={keptColor}
          strokeWidth="1.7"
          fill="none"
          opacity=".8"
        />
        <Arm n={keptN} cx={236} color={keptColor} />
        <circle cx="236" cy={GRAPH.root} r="4.2" fill={BASE} />
        <text x="248" y={armTop(keptN) - 10} fill={keptColor} fontSize="8.5">
          {count(keptLabel, keptN)}
        </text>
        <g opacity=".6">
          <circle cx="300" cy="150" r="4.2" fill="none" stroke={LOST} strokeWidth="1.5" strokeDasharray="2 2" />
          <circle cx="300" cy="126" r="4.2" fill="none" stroke={LOST} strokeWidth="1.5" strokeDasharray="2 2" />
          <path d="M292 138 L308 138" stroke={LOST} strokeWidth="1.3" opacity=".7" />
          <text x="264" y="172" fill={LOST} fontSize="8">
            {count(lostLabel, lostN)} deleted
          </text>
        </g>
      </>
    )
  }

  // Merge: both arms survive and rejoin at a new commit.
  if (mode === 'blend') {
    const yTop = armTop(ours)
    const tTop = armTop(theirs)
    const apex = Math.min(yTop, tTop) - 34
    return (
      <>
        <path
          d={`M236 ${GRAPH.root} L236 146 C236 134 210 134 210 122 L210 ${yTop - 14}`}
          stroke={OURS}
          strokeWidth="1.7"
          fill="none"
          opacity=".8"
        />
        <path
          d={`M236 ${GRAPH.root} L236 146 C236 134 268 134 268 122 L268 ${tTop - 14}`}
          stroke={THEIRS}
          strokeWidth="1.7"
          fill="none"
          opacity=".8"
        />
        <path
          d={`M210 ${yTop - 14} C210 ${apex + 12} 236 ${apex + 16} 236 ${apex}`}
          stroke={OURS}
          strokeWidth="1.7"
          fill="none"
          opacity=".8"
        />
        <path
          d={`M268 ${tTop - 14} C268 ${apex + 12} 236 ${apex + 16} 236 ${apex}`}
          stroke={THEIRS}
          strokeWidth="1.7"
          fill="none"
          opacity=".8"
        />
        <Arm n={ours} cx={210} color={OURS} />
        <Arm n={theirs} cx={268} color={THEIRS} />
        <circle cx="236" cy={GRAPH.root} r="4.2" fill={BASE} />
        <circle cx="236" cy={apex} r="5.4" fill={NEW} />
        <text x="236" y={apex - 10} fill={NEW} fontSize="8.5" textAnchor="middle">
          blend commit
        </text>
      </>
    )
  }

  // Rebase: one lane holding both arms, ours re-created on top with new IDs.
  if (mode === 'stack') {
    const tk = dotCount(theirs)
    const yk = dotCount(ours)
    const total = tk + yk
    const lane = stackedLane(total)
    return (
      <>
        <path
          d={`M236 172 L236 ${lane.y(total - 1) - 6}`}
          stroke={THEIRS}
          strokeWidth="1.7"
          fill="none"
          opacity=".8"
        />
        <circle cx="236" cy="172" r="4.2" fill={BASE} />
        {Array.from({ length: tk }, (_, i) => (
          <circle key={`t${i}`} cx="236" cy={lane.y(i)} r="4.2" fill={THEIRS} />
        ))}
        <text x="248" y={lane.y(0) + 3} fill={SUB} fontSize="8.5">
          {count(theirLabel, theirs)}
        </text>
        {Array.from({ length: yk }, (_, i) => (
          <circle
            key={`o${i}`}
            cx="236"
            cy={lane.y(tk + i)}
            r="4.6"
            fill="var(--gw-bg, #121212)"
            stroke={OURS}
            strokeWidth="1.9"
          />
        ))}
        <text x="248" y={lane.y(total - 1) - 1} fill={OURS} fontSize="8.5">
          {count(ourLabel, ours)}, rebuilt
        </text>
        <text x="248" y={lane.y(total - 1) + 10} fill={BASE} fontSize="8" fontFamily="var(--font-mono)">
          new IDs
        </text>
      </>
    )
  }

  // get / send: the same straight lane, now on both sides. Every commit lands,
  // so this draws the full arm -- showing fewer made a clean catch-up look like
  // it dropped work.
  const n = ours || theirs
  const color = ours ? OURS : THEIRS
  const label = ours ? ourLabel : theirLabel
  return (
    <>
      <path
        d={`M236 ${GRAPH.root} L236 ${armTop(n) - 14}`}
        stroke={color}
        strokeWidth="1.7"
        fill="none"
        opacity=".8"
      />
      <Arm n={n} cx={236} color={color} />
      <circle cx="236" cy={GRAPH.root} r="4.2" fill={BASE} />
      <text x="248" y={armTop(n) - 10} fill={color} fontSize="8.5">
        {count(label, n)}
      </text>
      <text x="248" y={armTop(n) + 1} fill={BASE} fontSize="8">
        {mode === 'get' ? 'now yours too' : 'now in the cloud'}
      </text>
    </>
  )
}
