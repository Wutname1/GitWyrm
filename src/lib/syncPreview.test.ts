import { describe, expect, it } from 'vitest'
import { armTop, dotCount, modeCopy, modesFor, stackedLane } from './syncPreview'

describe('modesFor', () => {
  it('offers the real three-way choice when both sides moved', () => {
    expect(modesFor({ ours: 2, theirs: 2 })).toEqual(['replace', 'blend', 'stack'])
  })

  it('offers only "get" when nothing of ours is in the way', () => {
    // A straight catch-up has no decision to make -- three buttons would imply
    // a choice that does not exist.
    expect(modesFor({ ours: 0, theirs: 4 })).toEqual(['get'])
  })

  it('offers only "send" when the cloud has nothing we lack', () => {
    expect(modesFor({ ours: 3, theirs: 0 })).toEqual(['send'])
  })

  it('offers nothing when the two already match', () => {
    expect(modesFor({ ours: 0, theirs: 0 })).toEqual([])
  })
})

describe('modeCopy', () => {
  const d = { ours: 2, theirs: 4 }

  it('names THEIR count as lost when replacing', () => {
    const c = modeCopy('replace', d)
    expect(c.danger).toBe(true)
    expect(c.sub).toBe('4 lost')
    expect(c.pill.tone).toBe('bad')
    expect(c.note.text).toContain('4 changes')
  })

  it('names OUR count as lost when resetting, the mirror of replace', () => {
    const c = modeCopy('reset', d)
    expect(c.danger).toBe(true)
    expect(c.sub).toBe('2 lost')
    expect(c.note.text).toContain('2 changes')
  })

  it('never says a local branch could have been pulled by someone else', () => {
    // You cannot pull someone's local copy; the real hazard is that the commits
    // were already pushed, so the rewrite makes the cloud copy disagree.
    const c = modeCopy('stack', d)
    expect(c.note.text).not.toMatch(/pulled/i)
    expect(c.note.text).toContain('sent them up')
  })

  it('tells the user what to weigh, not who might be inconvenienced', () => {
    const c = modeCopy('replace', d)
    expect(c.note.text).not.toMatch(/nobody else/i)
    expect(c.note.text).toContain("aren't needed")
  })

  it('marks the non-destructive options as losing nothing', () => {
    for (const mode of ['blend', 'stack', 'get', 'send'] as const) {
      const c = modeCopy(mode, d)
      expect(c.danger).toBe(false)
      expect(c.pill.text).toBe('nothing lost')
    }
  })

  it('uses singular wording for a single commit', () => {
    expect(modeCopy('stack', { ours: 1, theirs: 3 }).note.text).toContain('1 change ')
    expect(modeCopy('get', { ours: 0, theirs: 1 }).action).toBe('Get 1 change')
  })
})

describe('graph geometry', () => {
  it('caps the dots per arm so 40 commits draw like 3', () => {
    expect(dotCount(2)).toBe(2)
    expect(dotCount(40)).toBe(3)
    expect(armTop(40)).toBe(armTop(3))
  })

  it('keeps a tall stack clear of the heading', () => {
    // 3 ours + 40 theirs is the shape that used to run off the top of the
    // canvas and hide the "yours, rebuilt" label behind the AFTER heading.
    const lane = stackedLane(dotCount(3) + dotCount(40))
    const top = lane.y(dotCount(3) + dotCount(40) - 1)
    expect(top).toBeGreaterThan(20)
  })

  it('leaves small stacks at the full spacing', () => {
    const lane = stackedLane(2)
    expect(lane.gap).toBe(28)
  })
})
