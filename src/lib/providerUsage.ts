/**
 * Plain-language formatting for plan limits and model prices in Agent Desk.
 *
 * The numbers come from each AI tool's own account (Claude's 5-hour and weekly
 * windows, Codex's, Copilot's monthly premium requests); this file only turns
 * them into words and decides how worried a ring should look.
 */
import type { ProviderUsage, UsageWindow } from '@/lib/bindings'

export type UsageTone = 'ok' | 'warn' | 'high'

/** Calm below 70%, a warning from 70%, alarming from 90%. */
export function usageTone(percent: number): UsageTone {
  if (percent >= 90) return 'high'
  if (percent >= 70) return 'warn'
  return 'ok'
}

/** The window closest to running out, or null when the tool reported none. */
export function fullestWindow(usage: ProviderUsage | null | undefined): UsageWindow | null {
  if (!usage) return null
  let top: UsageWindow | null = null
  for (const w of usage.windows) {
    if (!top || w.usedPercent > top.usedPercent) top = w
  }
  return top
}

/** "62% used", never more than 100 or less than 0. */
export function formatUsedPercent(percent: number): string {
  return `${Math.round(Math.min(100, Math.max(0, percent)))}% used`
}

/**
 * When a window starts over, in words: "Resets in 2 hr 14 min", "Resets in
 * 3 days", or a date once it is a week or more away. `null` when unknown.
 */
export function formatResetsIn(resetsAt: string | null | undefined, now: Date = new Date()): string | null {
  if (!resetsAt) return null
  const at = new Date(resetsAt)
  const ms = at.getTime() - now.getTime()
  if (Number.isNaN(ms)) return null
  if (ms <= 60_000) return 'Resets in a moment'
  const minutes = Math.round(ms / 60_000)
  if (minutes < 60) return `Resets in ${minutes} min`
  const hours = Math.floor(minutes / 60)
  if (hours < 24) {
    const rest = minutes % 60
    return rest === 0 ? `Resets in ${hours} hr` : `Resets in ${hours} hr ${rest} min`
  }
  const days = Math.round(hours / 24)
  if (days < 7) return `Resets in ${days} day${days === 1 ? '' : 's'}`
  return `Resets ${at.toLocaleDateString(undefined, { month: 'short', day: 'numeric' })}`
}

/** "400K", "1.2M": a context window at a glance. */
export function formatTokenCount(tokens: number): string {
  if (tokens >= 1_000_000) return `${trimZero((tokens / 1_000_000).toFixed(1))}M`
  if (tokens >= 1_000) return `${Math.round(tokens / 1_000)}K`
  return String(tokens)
}

/** Credits per 1M tokens with thousands separators, keeping small fractions. */
export function formatCredits(value: number): string {
  if (value >= 100) return Math.round(value).toLocaleString('en-US')
  return trimZero(value.toFixed(value >= 10 ? 1 : 2))
}

/** Copilot's premium-request multiplier: "1x", "0.33x", or "Included" at 0. */
export function formatMultiplier(multiplier: number): string {
  if (multiplier === 0) return 'Included'
  return `${trimZero(multiplier.toFixed(2))}x`
}

function trimZero(text: string): string {
  return text.includes('.') ? text.replace(/0+$/, '').replace(/\.$/, '') : text
}
