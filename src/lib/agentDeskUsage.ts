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
      label: 'Estimated cost',
      value: `$${usage.sessionCostUsd.value.toFixed(2)}`,
      isEstimate: isEstimate(usage.sessionCostUsd),
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
