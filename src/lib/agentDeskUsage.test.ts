import { describe, expect, it } from 'vitest'
import { buildAgentUsageLines, buildUsageRows, hasAnyUsageData } from './agentDeskUsage'
import type { AgentUsageRow, SessionUsage } from '@/lib/bindings'

const EMPTY: SessionUsage = {
  sessionTokens: null,
  sessionRequests: null,
  sessionCostUsd: null,
  planLimit: null,
  planResetAt: null,
  activeHelperCount: null,
  contextUsed: null,
  contextSize: null,
  agents: [],
  dataTimestamp: '2026-08-19T00:00:00Z',
}

const LEAD: AgentUsageRow = {
  executionId: 'lead-1',
  label: 'Lead',
  isLead: true,
  tokens: 1200,
  costMicroUsd: 4500,
  turns: 3,
}

const HELPER: AgentUsageRow = {
  executionId: 'helper-1',
  label: 'Trace the crash',
  isLead: false,
  tokens: 500,
  costMicroUsd: null,
  turns: 1,
}

describe('buildAgentUsageLines', () => {
  it('shows nothing when a lone lead would only repeat the totals', () => {
    expect(buildAgentUsageLines({ ...EMPTY, agents: [LEAD] })).toEqual([])
    expect(buildAgentUsageLines(EMPTY)).toEqual([])
    // Older session files predate the field entirely.
    expect(buildAgentUsageLines({ ...EMPTY, agents: undefined })).toEqual([])
  })

  it('lists every agent once a helper exists', () => {
    const lines = buildAgentUsageLines({ ...EMPTY, agents: [LEAD, HELPER] })
    expect(lines.map((l) => l.label)).toEqual(['Lead', 'Trace the crash'])
    expect(lines[0].parts).toEqual(['1.2k tokens', '3 turns', '$0.0045'])
  })

  it('shows a lone helper too, since the lead may not have reported yet', () => {
    expect(buildAgentUsageLines({ ...EMPTY, agents: [HELPER] })).toHaveLength(1)
  })

  it('never invents a zero for a figure an agent did not report', () => {
    const lines = buildAgentUsageLines({ ...EMPTY, agents: [LEAD, HELPER] })
    expect(lines[1].parts).toEqual(['500 tokens', '1 turn'])
    const silent: AgentUsageRow = { ...HELPER, executionId: 'helper-2', tokens: null, turns: null }
    const quiet = buildAgentUsageLines({ ...EMPTY, agents: [LEAD, silent] })
    expect(quiet[1].parts).toEqual([])
  })
})

describe('buildUsageRows', () => {
  it('produces no rows at all when every field is unknown', () => {
    expect(buildUsageRows(EMPTY)).toEqual([])
    expect(hasAnyUsageData(EMPTY)).toBe(false)
  })

  it('never shows a zero for a field that was never measured', () => {
    // Only activeHelperCount is present (as a real, measured zero) -- every
    // other field stays absent and must not appear as "0" rows.
    const usage: SessionUsage = { ...EMPTY, activeHelperCount: 0 }
    const rows = buildUsageRows(usage)
    expect(rows).toHaveLength(1)
    expect(rows[0].key).toBe('helpers')
    expect(rows[0].value).toBe('0')
  })

  it('combines session tokens and requests into one row when both are present', () => {
    const usage: SessionUsage = {
      ...EMPTY,
      sessionTokens: { value: 31000, source: 'measured' },
      sessionRequests: { value: 7, source: 'measured' },
    }
    const rows = buildUsageRows(usage)
    expect(rows).toHaveLength(1)
    expect(rows[0].label).toBe('Current session')
    expect(rows[0].value).toContain('31k tokens')
    expect(rows[0].value).toContain('7 turns')
    expect(rows[0].isEstimate).toBe(false)
  })

  it('shows tokens alone when requests are not measured', () => {
    const usage: SessionUsage = { ...EMPTY, sessionTokens: { value: 1200, source: 'measured' } }
    const rows = buildUsageRows(usage)
    expect(rows).toHaveLength(1)
    expect(rows[0].value).toBe('1.2k tokens')
  })

  it('marks a row as an estimate when its source is estimated', () => {
    const usage: SessionUsage = {
      ...EMPTY,
      sessionCostUsd: { value: 0.42, source: 'estimated' },
    }
    const rows = buildUsageRows(usage)
    expect(rows).toHaveLength(1)
    expect(rows[0].isEstimate).toBe(true)
    expect(rows[0].value).toBe('$0.42')
  })

  it('does not mark a measured or provider-reported row as an estimate', () => {
    const usage: SessionUsage = {
      ...EMPTY,
      planLimit: { value: 2409, source: 'providerReported' },
    }
    const rows = buildUsageRows(usage)
    expect(rows[0].isEstimate).toBe(false)
  })

  it('calls a provider-reported cost a cost, not an estimate', () => {
    const usage: SessionUsage = {
      ...EMPTY,
      sessionCostUsd: { value: 0.42, source: 'providerReported' },
    }
    const rows = buildUsageRows(usage)
    expect(rows[0].label).toBe('Cost')
    expect(rows[0].isEstimate).toBe(false)
  })

  it('shows a sub-cent cost instead of rounding it away to $0.00', () => {
    // A single turn routinely costs well under a cent. Two decimal places
    // would render a real, provider-reported figure as free.
    const usage: SessionUsage = {
      ...EMPTY,
      sessionCostUsd: { value: 0.0034, source: 'providerReported' },
    }
    expect(buildUsageRows(usage)[0].value).toBe('$0.0034')
  })

  it('never rounds a real charge down to nothing', () => {
    // Cost arrives in millionths of a dollar, so four decimal places is not
    // enough on its own -- anything under $0.00005 would print "$0.0000",
    // which is the same "a real charge shown as free" bug in a new place.
    const usage: SessionUsage = {
      ...EMPTY,
      sessionCostUsd: { value: 0.000004, source: 'providerReported' },
    }
    expect(buildUsageRows(usage)[0].value).toBe('< $0.0001')
  })

  it('shows a genuine zero cost as $0.00 rather than padding it', () => {
    const usage: SessionUsage = {
      ...EMPTY,
      sessionCostUsd: { value: 0, source: 'providerReported' },
    }
    expect(buildUsageRows(usage)[0].value).toBe('$0.00')
  })

  it('reports session tokens the provider counted', () => {
    const usage: SessionUsage = {
      ...EMPTY,
      sessionTokens: { value: 31_400, source: 'providerReported' },
      sessionRequests: { value: 7, source: 'measured' },
    }
    const rows = buildUsageRows(usage)
    expect(rows[0].label).toBe('Current session')
    expect(rows[0].value).toBe('31k tokens · 7 turns')
    expect(rows[0].isEstimate).toBe(false)
  })

  it('shows context as a share of the window rather than raw numbers', () => {
    // "31k of 200k" makes the reader do the division to answer the only
    // question they have, which is how close a compaction is.
    const usage: SessionUsage = {
      ...EMPTY,
      contextUsed: { value: 31_000, source: 'providerReported' },
      contextSize: { value: 200_000, source: 'providerReported' },
    }
    const rows = buildUsageRows(usage)
    expect(rows[0].label).toBe('Context used')
    expect(rows[0].value).toBe('16% of 200k tokens')
  })

  it('shows no context row when only half of it was reported', () => {
    // A window size with no occupancy, or the reverse, says nothing useful.
    expect(
      buildUsageRows({ ...EMPTY, contextSize: { value: 200_000, source: 'providerReported' } })
    ).toEqual([])
  })

  it('does not divide by a zero window', () => {
    const usage: SessionUsage = {
      ...EMPTY,
      contextUsed: { value: 10, source: 'providerReported' },
      contextSize: { value: 0, source: 'providerReported' },
    }
    expect(buildUsageRows(usage)).toEqual([])
  })

  it('formats the reset date without inventing a time-of-day claim', () => {
    const usage: SessionUsage = { ...EMPTY, planResetAt: '2026-09-01T00:00:00Z' }
    const rows = buildUsageRows(usage)
    expect(rows).toHaveLength(1)
    expect(rows[0].label).toBe('Resets')
  })
})
