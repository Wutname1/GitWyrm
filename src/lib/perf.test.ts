import { describe, expect, it } from 'vitest'
import { nextGraphLoadAction } from './perf'

/**
 * The graph-load measurement is a small state machine over (repoId, isLoading)
 * plus whichever repo currently has an open span. Its failure mode is a
 * *sequence*, not a single call: 0.5.2 reported `graph.load` spans lasting
 * tens of millions of milliseconds because switching repos mid-load stranded a
 * span that could never reach its finish condition.
 */
describe('nextGraphLoadAction', () => {
  it('starts a measurement when a repo begins loading', () => {
    expect(nextGraphLoadAction('a', true, null)).toBe('start')
  })

  it('finishes the measurement it opened', () => {
    expect(nextGraphLoadAction('a', false, 'a')).toBe('finish')
  })

  it('does nothing for a repo that was never loading', () => {
    expect(nextGraphLoadAction('a', false, null)).toBe('none')
  })

  it('does not reopen a span while the same load is still running', () => {
    expect(nextGraphLoadAction('a', true, 'a')).toBe('none')
  })

  /**
   * The regression. Switching to repo b while a is still loading must abandon
   * a's span. Before the fix this returned no action, so a's span stayed open
   * until the component unmounted -- hours in a desktop app.
   */
  it('abandons a stranded span when the repo changes mid-load', () => {
    expect(nextGraphLoadAction('b', true, 'a')).toBe('abandon')
  })

  it('abandons a stranded span even if the new repo is already loaded', () => {
    expect(nextGraphLoadAction('b', false, 'a')).toBe('abandon')
  })

  it('abandons a stranded span when every repo closes', () => {
    expect(nextGraphLoadAction(null, false, 'a')).toBe('abandon')
  })

  it('stays quiet with no repo and nothing open', () => {
    expect(nextGraphLoadAction(null, false, null)).toBe('none')
  })

  /** A full switch-mid-load sequence never leaves a span open. */
  it('never strands a span across a mid-load repo switch', () => {
    let open: string | null = null
    const apply = (repoId: string | null, isLoading: boolean) => {
      const action = nextGraphLoadAction(repoId, isLoading, open)
      if (action === 'abandon') open = null
      if (action === 'start') open = repoId
      if (action === 'finish') open = null
      // An abandon that lands on a still-loading repo re-opens for it, matching
      // the hook: the user is now waiting on this repo instead.
      if (action === 'abandon' && isLoading && repoId != null) open = repoId
      return action
    }

    expect(apply('a', true)).toBe('start')
    expect(open).toBe('a')
    // Switch to b before a ever finishes.
    expect(apply('b', true)).toBe('abandon')
    expect(open).toBe('b')
    // b finishes normally, leaving nothing behind.
    expect(apply('b', false)).toBe('finish')
    expect(open).toBeNull()
  })
})
