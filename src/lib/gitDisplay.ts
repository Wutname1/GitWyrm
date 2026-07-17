import type { StatusCode } from './bindings'

export const LANE_COLORS = ['var(--gw-lane0)', 'var(--gw-lane1)', 'var(--gw-lane2)'] as const

export const GRAPH_ROW_HEIGHT = 28

export function laneColor(lane: number): string {
  return LANE_COLORS[lane % LANE_COLORS.length]
}

export function statusColor(st: StatusCode): string {
  if (st === 'A') return 'var(--gw-green)'
  if (st === 'D' || st === '!') return 'var(--gw-red)'
  return 'var(--gw-amber)'
}

const AUTHOR_PALETTE = ['#2dd4a7', '#38bdf8', '#f59e0b', '#e06b9a', '#a78bfa', '#fb7185', '#4ade80']

/** Deterministic color per author (keyed by email or name). */
export function authorColor(key: string): string {
  let hash = 0
  for (let i = 0; i < key.length; i++) {
    hash = (hash * 31 + key.charCodeAt(i)) | 0
  }
  return AUTHOR_PALETTE[Math.abs(hash) % AUTHOR_PALETTE.length]
}

/** "Jul 16 14:22" style, matching the design. */
export function formatCommitTime(epochSeconds: number): string {
  const d = new Date(epochSeconds * 1000)
  const month = d.toLocaleString('en-US', { month: 'short' })
  const hh = String(d.getHours()).padStart(2, '0')
  const mm = String(d.getMinutes()).padStart(2, '0')
  return `${month} ${d.getDate()} ${hh}:${mm}`
}
