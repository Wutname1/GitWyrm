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

/** Short relative age: "just now", "4h ago", "3d ago", "2mo ago". */
export function formatRelativeTime(epochSeconds: number, now = Date.now()): string {
  const secs = Math.max(0, Math.floor(now / 1000 - epochSeconds))
  if (secs < 60) return 'just now'
  const mins = Math.floor(secs / 60)
  if (mins < 60) return `${mins}m ago`
  const hours = Math.floor(mins / 60)
  if (hours < 24) return `${hours}h ago`
  const days = Math.floor(hours / 24)
  if (days < 30) return `${days}d ago`
  const months = Math.floor(days / 30)
  if (months < 12) return `${months}mo ago`
  return `${Math.floor(months / 12)}y ago`
}

/** "Jul 16 14:22" style, matching the design. */
export function formatCommitTime(epochSeconds: number): string {
  const d = new Date(epochSeconds * 1000)
  const month = d.toLocaleString('en-US', { month: 'short' })
  const hh = String(d.getHours()).padStart(2, '0')
  const mm = String(d.getMinutes()).padStart(2, '0')
  return `${month} ${d.getDate()} ${hh}:${mm}`
}

/**
 * "1 commit" / "2 commits". Pass `many` for irregular plurals
 * (`plural(n, 'repository', 'repositories')`).
 */
export function plural(n: number, one: string, many = `${one}s`): string {
  return `${n} ${n === 1 ? one : many}`
}

/** A commit id shortened for display, matching the 7 chars git uses. */
export function shortSha(sha: string): string {
  return sha.slice(0, 7)
}
