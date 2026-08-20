import { describe, expect, it } from 'vitest'
import { AUTO_FOLLOW_THRESHOLD_PX, isNearBottom } from './agentDeskScroll'

describe('isNearBottom', () => {
  it('is true for an empty transcript that has not overflowed yet', () => {
    expect(isNearBottom({ scrollTop: 0, scrollHeight: 200, clientHeight: 400 })).toBe(true)
  })

  it('is true when scrolled exactly to the bottom', () => {
    expect(isNearBottom({ scrollTop: 600, scrollHeight: 1000, clientHeight: 400 })).toBe(true)
  })

  it('is true within the threshold of the bottom', () => {
    const unseen = AUTO_FOLLOW_THRESHOLD_PX - 1
    expect(
      isNearBottom({ scrollTop: 1000 - 400 - unseen, scrollHeight: 1000, clientHeight: 400 })
    ).toBe(true)
  })

  it('is false once scrolled further than the threshold from the bottom', () => {
    const unseen = AUTO_FOLLOW_THRESHOLD_PX + 1
    expect(
      isNearBottom({ scrollTop: 1000 - 400 - unseen, scrollHeight: 1000, clientHeight: 400 })
    ).toBe(false)
  })

  it('is false when scrolled all the way to the top of a long transcript', () => {
    expect(isNearBottom({ scrollTop: 0, scrollHeight: 5000, clientHeight: 400 })).toBe(false)
  })
})
