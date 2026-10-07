import { describe, expect, it } from 'vitest'
import {
  formatCredits,
  formatMultiplier,
  formatResetsIn,
  formatTokenCount,
  formatUsedPercent,
  fullestWindow,
  usageTone,
} from './providerUsage'

const NOW = new Date('2026-10-06T12:00:00Z')
const later = (ms: number) => new Date(NOW.getTime() + ms).toISOString()

describe('provider usage formatting', () => {
  it('turns a percent into a tone', () => {
    expect(usageTone(10)).toBe('ok')
    expect(usageTone(70)).toBe('warn')
    expect(usageTone(95)).toBe('high')
  })

  it('picks the window closest to its limit', () => {
    const usage = {
      provider: 'claude',
      displayName: 'Claude Code',
      plan: null,
      fetchedAt: null,
      unavailableReason: null,
      windows: [
        { label: '5-hour limit', kind: 'fiveHour' as const, usedPercent: 40, resetsAt: null, detail: null },
        { label: 'Weekly limit', kind: 'weekly' as const, usedPercent: 81, resetsAt: null, detail: null },
      ],
    }
    expect(fullestWindow(usage)?.label).toBe('Weekly limit')
    expect(fullestWindow(null)).toBeNull()
  })

  it('clamps the percent it shows', () => {
    expect(formatUsedPercent(104)).toBe('100% used')
    expect(formatUsedPercent(-3)).toBe('0% used')
  })

  it('says when a window resets in plain words', () => {
    expect(formatResetsIn(later(30_000), NOW)).toBe('Resets in a moment')
    expect(formatResetsIn(later(45 * 60_000), NOW)).toBe('Resets in 45 min')
    expect(formatResetsIn(later(2 * 3_600_000 + 14 * 60_000), NOW)).toBe('Resets in 2 hr 14 min')
    expect(formatResetsIn(later(3 * 3_600_000), NOW)).toBe('Resets in 3 hr')
    expect(formatResetsIn(later(3 * 86_400_000), NOW)).toBe('Resets in 3 days')
    expect(formatResetsIn(later(20 * 86_400_000), NOW)).toMatch(/^Resets [A-Z][a-z]{2} \d+$/)
    expect(formatResetsIn(null, NOW)).toBeNull()
    expect(formatResetsIn('not a date', NOW)).toBeNull()
  })

  it('formats context windows, credits and multipliers', () => {
    expect(formatTokenCount(400_000)).toBe('400K')
    expect(formatTokenCount(1_200_000)).toBe('1.2M')
    expect(formatTokenCount(1_000_000)).toBe('1M')
    expect(formatCredits(5000)).toBe('5,000')
    expect(formatCredits(12.5)).toBe('12.5')
    expect(formatCredits(0.25)).toBe('0.25')
    expect(formatMultiplier(1)).toBe('1x')
    expect(formatMultiplier(0.33)).toBe('0.33x')
    expect(formatMultiplier(0)).toBe('Included')
  })
})
