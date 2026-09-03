import { describe, expect, it } from 'vitest'
import type { SpecReturnTarget } from '@/lib/bindings'
import { specReturnTargets } from './agentDeskSpecReturn'

describe('specReturnTargets', () => {
  it('covers every file the backend can draft, once each', () => {
    // If the backend gains a fourth target, this fails rather than the menu
    // quietly omitting it.
    const ids = specReturnTargets.map((t) => t.id)
    const expected: SpecReturnTarget[] = ['tasks', 'proposal', 'design']
    expect([...ids].sort()).toEqual([...expected].sort())
    expect(new Set(ids).size).toBe(ids.length)
  })

  it('leads with the task list, which goes stale first', () => {
    expect(specReturnTargets[0].id).toBe('tasks')
  })

  it('says what each one does in plain words, with no jargon', () => {
    for (const target of specReturnTargets) {
      expect(target.label.length).toBeGreaterThan(0)
      expect(target.detail.length).toBeGreaterThan(0)
      // The menu is read before anything is drafted, so it must never
      // suggest something is being written.
      expect(target.label.toLowerCase()).not.toContain('write')
      expect(target.detail).not.toMatch(/—/)
    }
  })
})
