import { useMemo } from 'react'
import type { CommitEntry } from '@/lib/bindings'
import { GRAPH_ROW_HEIGHT, laneColor } from '@/lib/gitDisplay'

const X0 = 16
const LANE_WIDTH = 20

const laneX = (lane: number) => X0 + lane * LANE_WIDTH
const rowCenterY = (index: number) => index * GRAPH_ROW_HEIGHT + GRAPH_ROW_HEIGHT / 2

interface GraphSvgProps {
  commits: CommitEntry[]
  selectedSha: string | null
  /** Virtualized visible window (inclusive indices). */
  startIndex: number
  endIndex: number
}

/**
 * Renders edges from each commit to its parents using the backend-computed
 * lanes. Edge endpoints use the parent's row index (found via sha lookup);
 * parents beyond the loaded pages get a short fading stub.
 */
export function GraphSvg({ commits, selectedSha, startIndex, endIndex }: GraphSvgProps) {
  const indexBySha = useMemo(() => {
    const m = new Map<string, number>()
    commits.forEach((c, i) => m.set(c.sha, i))
    return m
  }, [commits])

  const edges = useMemo(() => {
    const out: { d: string; color: string; fade?: boolean }[] = []
    const lo = Math.max(0, startIndex - 30)
    const hi = Math.min(commits.length - 1, endIndex + 30)
    for (let i = lo; i <= hi; i++) {
      const c = commits[i]
      c.parent_shas.forEach((parentSha, pi) => {
        const parentIndex = indexBySha.get(parentSha)
        const x1 = laneX(c.lane)
        const y1 = rowCenterY(i)
        const parentLane = c.parent_lanes[pi] ?? c.lane
        if (parentIndex == null) {
          // Parent not loaded yet: draw a stub downward.
          out.push({
            d: `M ${x1} ${y1} L ${laneX(parentLane)} ${y1 + GRAPH_ROW_HEIGHT}`,
            color: laneColor(parentLane),
            fade: true,
          })
          return
        }
        const x2 = laneX(commits[parentIndex].lane)
        const y2 = rowCenterY(parentIndex)
        const d =
          x1 === x2
            ? `M ${x1} ${y1} L ${x2} ${y2}`
            : `M ${x1} ${y1} C ${x1} ${y1 + (y2 - y1) * 0.5}, ${x2} ${y1 + (y2 - y1) * 0.5}, ${x2} ${y2}`
        out.push({ d, color: laneColor(Math.max(c.lane, commits[parentIndex].lane)) })
      })
    }
    return out
  }, [commits, indexBySha, startIndex, endIndex])

  return (
    <svg
      width={96}
      height={commits.length * GRAPH_ROW_HEIGHT}
      className="pointer-events-none absolute left-[150px] top-0 overflow-visible"
    >
      {edges.map((e, i) => (
        <path
          key={i}
          d={e.d}
          fill="none"
          stroke={e.color}
          strokeWidth={2.6}
          strokeLinecap="round"
          opacity={e.fade ? 0.35 : 1}
        />
      ))}
      {commits.map((c, i) => {
        if (i < startIndex || i > endIndex) return null
        const sel = selectedSha === c.sha
        const col = laneColor(c.lane)
        return (
          <circle
            key={c.sha}
            cx={laneX(c.lane)}
            cy={rowCenterY(i)}
            r={sel ? 7.5 : 6}
            fill={c.is_merge ? 'var(--gw-bg)' : col}
            stroke={sel ? 'var(--gw-text)' : c.is_merge ? col : 'var(--gw-bg)'}
            strokeWidth={sel ? 2.5 : c.is_merge ? 2.5 : 2}
          />
        )
      })}
    </svg>
  )
}
