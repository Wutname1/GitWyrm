import { describe, expect, it } from 'vitest'
import { buildUsageRows, hasAnyUsageData } from './agentDeskUsage'
import type { SessionUsage } from '@/lib/bindings'

const EMPTY: SessionUsage = {
  sessionTokens: null,
  sessionRequests: null,
  sessionCostUsd: null,
  planLimit: null,
  planResetAt: null,
  activeHelperCount: null,
  dataTimestamp: '2026-08-19T00:00:00Z',
}

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

  it('formats the reset date without inventing a time-of-day claim', () => {
    const usage: SessionUsage = { ...EMPTY, planResetAt: '2026-09-01T00:00:00Z' }
    const rows = buildUsageRows(usage)
    expect(rows).toHaveLength(1)
    expect(rows[0].label).toBe('Resets')
  })
})
