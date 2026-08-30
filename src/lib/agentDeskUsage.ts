import type { SessionUsage, UsageValue } from '@/lib/bindings'

/** One renderable row in the usage card: a label plus formatted value text. */
export interface UsageRow {
  key: string
  label: string
  value: string
  /** True when the value is `estimated`, so the UI can mark it as such. */
  isEstimate: boolean
}

function formatCount(n: number): string {
  return n.toLocaleString()
}

function formatTokens(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}m tokens`
  if (n >= 1_000) return `${(n / 1_000).toFixed(n >= 10_000 ? 0 : 1)}k tokens`
  return `${formatCount(n)} tokens`
}

function isEstimate(v: UsageValue): boolean {
  return v.source === 'estimated'
}

/**
 * Money, without ever rounding a real charge down to nothing.
 *
 * A single turn routinely costs a fraction of a cent, so plain two-decimal
 * formatting would print a genuine charge as "$0.00". Adding decimal places
 * only moves the problem: cost arrives in millionths of a dollar, so four
 * places still shows anything under $0.00005 as "$0.0000".
 *
 * So anything too small to write exactly becomes "< $0.0001" -- true, and
 * clearly not free. A real zero still prints "$0.00", because a run that cost
 * nothing should say so plainly rather than hedging.
 */
function formatCost(usd: number): string {
  if (usd <= 0) return '$0.00'
  if (usd < 0.0001) return '< $0.0001'
  if (usd < 0.01) return `$${usd.toFixed(4)}`
  return `$${usd.toFixed(2)}`
}

/**
 * Builds the usage card's rows from optional, provider-normalized data
 * (architecture.md section 12).
 *
 * Every field on `SessionUsage` is optional. This never invents a zero for a
 * missing field -- an absent value produces no row at all, rather than a row
 * reading "0" that would misrepresent "not measured" as "measured and empty"
 * (tasks.md 7.4). Rows carrying `isEstimate: true` are for the caller to
 * label as an estimate in the accessible details, per the same task.
 *
 * `sessionTokens` and `sessionRequests` are combined into one "Current
 * session" row when both are present, matching the mockup's single
 * `"31k tokens · 7 turns"` line; either one alone still renders on its own.
 */
export function buildUsageRows(usage: SessionUsage): UsageRow[] {
  const rows: UsageRow[] = []

  if (usage.sessionTokens && usage.sessionRequests) {
    rows.push({
      key: 'session',
      label: 'Current session',
      value: `${formatTokens(usage.sessionTokens.value)} · ${formatCount(usage.sessionRequests.value)} turn${usage.sessionRequests.value === 1 ? '' : 's'}`,
      isEstimate: isEstimate(usage.sessionTokens) || isEstimate(usage.sessionRequests),
    })
  } else if (usage.sessionTokens) {
    rows.push({
      key: 'session',
      label: 'Current session',
      value: formatTokens(usage.sessionTokens.value),
      isEstimate: isEstimate(usage.sessionTokens),
    })
  } else if (usage.sessionRequests) {
    rows.push({
      key: 'session',
      label: 'Current session',
      value: `${formatCount(usage.sessionRequests.value)} turn${usage.sessionRequests.value === 1 ? '' : 's'}`,
      isEstimate: isEstimate(usage.sessionRequests),
    })
  }

  if (usage.sessionCostUsd) {
    rows.push({
      key: 'cost',
      // "Estimated cost" only when it actually is one. A figure the provider
      // reported is not an estimate, and calling it one would undersell a
      // real number the same way inventing one would oversell an absent one.
      label: isEstimate(usage.sessionCostUsd) ? 'Estimated cost' : 'Cost',
      value: formatCost(usage.sessionCostUsd.value),
      isEstimate: isEstimate(usage.sessionCostUsd),
    })
  }

  // Occupancy, shown as a share of the window rather than a raw pair of
  // numbers: "31k of 200k" makes someone do the division to answer the only
  // question they actually have, which is how close a compaction is.
  if (usage.contextUsed && usage.contextSize && usage.contextSize.value > 0) {
    const pct = Math.round((usage.contextUsed.value / usage.contextSize.value) * 100)
    rows.push({
      key: 'context',
      label: 'Context used',
      value: `${pct}% of ${formatTokens(usage.contextSize.value)}`,
      isEstimate: isEstimate(usage.contextUsed),
    })
  }

  if (usage.planLimit) {
    const remaining = Math.max(0, usage.planLimit.value)
    rows.push({
      key: 'planLimit',
      label: 'Premium interactions',
      value: `${formatCount(remaining)} left`,
      isEstimate: isEstimate(usage.planLimit),
    })
  }

  if (usage.activeHelperCount != null) {
    rows.push({
      key: 'helpers',
      label: 'Helpers active',
      value: formatCount(usage.activeHelperCount),
      isEstimate: false,
    })
  }

  if (usage.planResetAt) {
    const t = Date.parse(usage.planResetAt)
    rows.push({
      key: 'resets',
      label: 'Resets',
      value: Number.isNaN(t)
        ? usage.planResetAt
        : new Date(t).toLocaleDateString(undefined, { weekday: 'short', month: 'short', day: 'numeric' }),
      isEstimate: false,
    })
  }

  return rows
}

/** True when there is nothing measured/reported to show at all. */
export function hasAnyUsageData(usage: SessionUsage): boolean {
  return buildUsageRows(usage).length > 0
}
