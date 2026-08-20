import { describe, expect, it } from 'vitest'
import { otherPane, resolvePaneTarget, resolveSplitCollapse } from './agentDeskPaneTargeting'

describe('resolvePaneTarget', () => {
  it('always targets primary when not split', () => {
    const result = resolvePaneTarget({
      split: false,
      activePane: 'secondary',
      primarySessionId: 'a',
      secondarySessionId: 'b',
      sessionId: 'c',
    })
    expect(result).toEqual({ action: 'open', pane: 'primary' })
  })

  it('opens into the active pane when split and the session is not already visible', () => {
    const result = resolvePaneTarget({
      split: true,
      activePane: 'secondary',
      primarySessionId: 'a',
      secondarySessionId: 'b',
      sessionId: 'c',
    })
    expect(result).toEqual({ action: 'open', pane: 'secondary' })
  })

  it('focuses the primary pane instead of duplicating when already shown there', () => {
    const result = resolvePaneTarget({
      split: true,
      activePane: 'secondary',
      primarySessionId: 'a',
      secondarySessionId: 'b',
      sessionId: 'a',
    })
    expect(result).toEqual({ action: 'focus', pane: 'primary' })
  })

  it('focuses the secondary pane instead of duplicating when already shown there', () => {
    const result = resolvePaneTarget({
      split: true,
      activePane: 'primary',
      primarySessionId: 'a',
      secondarySessionId: 'b',
      sessionId: 'b',
    })
    expect(result).toEqual({ action: 'focus', pane: 'secondary' })
  })
})

describe('resolveSplitCollapse', () => {
  it('keeps the primary session when primary is active', () => {
    const result = resolveSplitCollapse({ activePane: 'primary', primarySessionId: 'a', secondarySessionId: 'b' })
    expect(result).toEqual({ activePane: 'primary', primarySessionId: 'a', secondarySessionId: null })
  })

  it('promotes the secondary session into primary when secondary is active', () => {
    const result = resolveSplitCollapse({ activePane: 'secondary', primarySessionId: 'a', secondarySessionId: 'b' })
    expect(result).toEqual({ activePane: 'primary', primarySessionId: 'b', secondarySessionId: null })
  })

  it('handles a null secondary session when secondary is active', () => {
    const result = resolveSplitCollapse({ activePane: 'secondary', primarySessionId: 'a', secondarySessionId: null })
    expect(result).toEqual({ activePane: 'primary', primarySessionId: null, secondarySessionId: null })
  })
})

describe('otherPane', () => {
  it('flips primary to secondary and back', () => {
    expect(otherPane('primary')).toBe('secondary')
    expect(otherPane('secondary')).toBe('primary')
  })
})

describe('rapid selection (tasks.md 4.7)', () => {
  it('a run of 100 selections always ends on the last one chosen', () => {
    // Models the sidebar clicking loop: each result is fed back in as the
    // pane's new session, exactly as the view does. The point is that
    // targeting is a pure function of the *current* layout, so no earlier
    // (or slower) selection can be left holding the pane at the end.
    let primarySessionId: string | null = null
    let secondarySessionId: string | null = null
    let activePane: 'primary' | 'secondary' = 'primary'

    for (let i = 0; i < 100; i++) {
      const sessionId = `s${i}`
      const target = resolvePaneTarget({
        split: false,
        activePane,
        primarySessionId,
        secondarySessionId,
        sessionId,
      })
      activePane = target.pane
      if (target.action === 'open') {
        if (target.pane === 'primary') primarySessionId = sessionId
        else secondarySessionId = sessionId
      }
    }

    expect(primarySessionId).toBe('s99')
    expect(secondarySessionId).toBeNull()
  })

  it('re-selecting the session already shown never reopens it', () => {
    const input = {
      split: true,
      activePane: 'secondary' as const,
      primarySessionId: 'a',
      secondarySessionId: 'b',
    }
    // Ten clicks on the row that is already in the primary pane: every one
    // resolves to "focus primary", so the secondary pane is never
    // overwritten with a duplicate of it.
    for (let i = 0; i < 10; i++) {
      expect(resolvePaneTarget({ ...input, sessionId: 'a' })).toEqual({ action: 'focus', pane: 'primary' })
    }
  })
})
