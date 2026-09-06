import type { ExecutionUsage, SessionUsage, SessionUsageOutcome, UsageValue } from '@/lib/bindings'

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
      // "Premium interactions" was one AI tool's own billing term and
      // appeared nowhere else in the product. Someone who has never read
      // that tool's pricing page cannot tell what it counts.
      label: 'Allowance left',
      value: formatCount(remaining),
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

  // "How much of my allowance is left" is the question someone running a lot
  // of agent work actually has, and no AI tool GitWyrm talks to reports it
  // today. Omitting the row answered that question with silence, which reads
  // as "there is nothing to say" rather than "GitWyrm cannot see this" --
  // and silence is exactly what the unknown-stays-unknown rule exists to
  // prevent.
  //
  // Only worth saying beside figures that were actually reported. A chat
  // whose only row is "0 helpers active" has not run yet, and answering a
  // question nobody has asked there is clutter rather than honesty -- the
  // card's own empty state already covers a chat with nothing to show.
  //
  // Keyed off the rows built above rather than the raw fields, so a figure
  // that was reported but produced no row (a context reading against a zero
  // window, say) cannot drag this one onto an otherwise empty card.
  const hasReportedFigures = rows.some((r) => r.key !== 'helpers')
  if (!usage.planLimit && hasReportedFigures) {
    rows.push({
      key: 'planLimit',
      label: 'Allowance left',
      value: 'not reported',
      isEstimate: false,
    })
  }

  return rows
}

/**
 * True when there is anything measured or reported to show at all.
 *
 * Counts the per-agent lines as well as the session rows. It used to check
 * only the rows -- the same blind spot that let the card print "No usage data
 * yet for this chat" directly above a populated per-agent list, because the
 * two are computed from different fields and a provider can report one
 * without the other.
 */
export function hasAnyUsageData(usage: SessionUsage): boolean {
  return buildUsageRows(usage).length > 0 || buildAgentUsageLines(usage).length > 0
}

/** One per-agent line under the totals: who, and what they reported. */
export interface AgentUsageLine {
  key: string
  label: string
  /** Already-formatted figures, one per reported field, in display order. */
  parts: string[]
}

/**
 * The per-agent breakdown, or an empty list when it would only repeat the
 * totals. A single lead on its own IS the session total, so the breakdown
 * appears only once there are two or more agents or any helper at all.
 *
 * Same honesty rule as `buildUsageRows`: a field the provider did not report
 * produces no part, never a "0". An agent that reported nothing usable still
 * gets a line naming it, so the list matches what actually ran.
 */
export function buildAgentUsageLines(usage: SessionUsage): AgentUsageLine[] {
  const agents = usage.agents ?? []
  const hasHelper = agents.some((a) => !a.isLead)
  if (agents.length < 2 && !hasHelper) return []
  return agents.map((agent) => {
    const parts: string[] = []
    // Same rule as `nodeUsageLine`: name the half you know rather than adding
    // a zero for the other. The backend used to fold a half-known figure into
    // `tokens` and present it as a total; it now sends the halves.
    if (agent.tokens != null) parts.push(formatTokens(agent.tokens))
    else if (agent.inputTokens != null) parts.push(`${formatTokens(agent.inputTokens)} in`)
    else if (agent.outputTokens != null) parts.push(`${formatTokens(agent.outputTokens)} out`)
    if (agent.turns != null) parts.push(`${formatCount(agent.turns)} turn${agent.turns === 1 ? '' : 's'}`)
    if (agent.costMicroUsd != null) parts.push(formatCost(agent.costMicroUsd / 1_000_000))
    return { key: agent.executionId, label: agent.label, parts }
  })
}

/**
 * What one agent in a team spent, as a single line, or `null` when its
 * provider reported nothing.
 *
 * The whole argument for a lead with helpers is parallel work at an
 * acceptable cost -- and the graph showed a node's job, files, dependencies
 * and result but never what it spent. So the feature that costs the most
 * money had the least visible cost, and a person looking at an expensive run
 * could not tell WHICH helper was expensive, which is the fact that would let
 * them change the team shape next time.
 *
 * Follows the same rule as every other usage surface: a field the provider
 * did not report contributes nothing, rather than a zero that would read as
 * "measured, and free".
 */
export function nodeUsageLine(usage: ExecutionUsage | null | undefined): string | null {
  if (!usage) return null
  const parts: string[] = []
  // Only add up what was actually reported. `?? 0` on both meant a provider
  // that reported input but not output (each is looked up independently, over
  // several spellings, in `ai/agent/acp.rs`) had the missing half silently
  // counted as zero and folded into a figure shown as the node's total -- the
  // "absent is not zero" rule broken by the very line the doc above says
  // keeps it. When only one half is known, say which half it is.
  const haveIn = usage.inputTokens != null
  const haveOut = usage.outputTokens != null
  // The card's own formatter, not a second one. A local copy claimed in its
  // comment to match this and did not: it printed "1.5M" where the card
  // printed "1.5m", diverging only above a million -- so the two disagreed
  // exactly in the long runs where cost matters most, and the comment
  // discouraged anyone from checking.
  if (haveIn && haveOut) parts.push(formatTokens(usage.inputTokens! + usage.outputTokens!))
  // `formatTokens` already ends in "tokens", so the half-known cases say
  // which half it is by naming it up front rather than appending a suffix.
  else if (haveIn) parts.push(`${formatTokens(usage.inputTokens!)} in`)
  else if (haveOut) parts.push(`${formatTokens(usage.outputTokens!)} out`)
  // `> 0` here suppressed a REPORTED zero, while `buildAgentUsageLines` --
  // reading the very same `usage.turns` off the very same execution record --
  // printed "0 turns". So one helper was described two ways at once, in two
  // panels a person can have open together.
  //
  // The card is the correct half. A provider that reports zero has measured
  // something; hiding it is the "unknown vs zero" rule inverted, dropping a
  // real figure instead of inventing an absent one. Absence is still absence:
  // `!= null` is what keeps that.
  // Cached input, which both real provider paths report and nothing showed.
  //
  // A SUBSET of `inputTokens`, not an addition -- its own backend doc calls it
  // "the number that explains a surprisingly small bill". So it is never added
  // into the total above; it is named beside it, which is the only way it can
  // be read without inviting double-counting.
  //
  // Shown only alongside a known input figure: "800 cached" on its own would
  // be a fraction with no denominator.
  if (haveIn && usage.cachedInputTokens != null) {
    // `formatTokens` already ends in "tokens", so this reads "800 tokens
    // cached" rather than repeating the noun.
    parts.push(`${formatTokens(usage.cachedInputTokens)} cached`)
  }
  if (usage.turns != null) parts.push(`${usage.turns} turn${usage.turns === 1 ? '' : 's'}`)
  if (usage.costMicroUsd != null) parts.push(formatCost(usage.costMicroUsd / 1_000_000))
  return parts.length > 0 ? parts.join(' · ') : null
}

/**
 * Why the usage figures could not be read, when they could not.
 *
 * `SessionUsageOutcome` distinguishes "there is nothing recorded" from "the
 * file is damaged" and "GitWyrm could not read it right now", and the card
 * collapsed all three into `null` -- which it then rendered as "No usage data
 * yet for this chat." A failed read stated as an absence is the one inversion
 * this card exists to prevent: unknown must stay unknown, never zero, and
 * never a confident nothing.
 *
 * `notFound` returns null because it genuinely IS the empty case: nothing has
 * been recorded for this chat, which the plain empty state already says well.
 */
export function explainUsageUnavailable(
  outcome: SessionUsageOutcome | undefined,
  isError: boolean
): string | null {
  if (isError) return 'GitWyrm could not read this chat to work out what it cost.'
  if (!outcome) return null
  switch (outcome.kind) {
    case 'available':
    case 'notFound':
      return null
    case 'damaged':
      return `This chat's saved file could not be read, so its cost is unknown: ${outcome.reason}`
    case 'unavailable':
      return `The cost of this chat is unknown right now: ${outcome.detail}`
  }
}
