import { describe, expect, it } from 'vitest'
import {
  armTop,
  dotCount,
  initialMode,
  modeCopy,
  modesFor,
  pullNeedsChoice,
  shownModes,
  stackedLane,
} from './syncPreview'

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

describe('pullNeedsChoice', () => {
  const up = 'origin/main'

  it('asks first when both sides moved, instead of inventing a merge commit', () => {
    // The reported bug: 2 local and 2 remote, and pull silently merged.
    expect(pullNeedsChoice({ upstream: up, ahead: 2, behind: 2 })).toBe(true)
  })

  it('pulls straight through when only the cloud moved', () => {
    // A pure fast-forward has no history decision in it, so a prompt would
    // just be a click in the way.
    expect(pullNeedsChoice({ upstream: up, ahead: 0, behind: 4 })).toBe(false)
  })

  it('pulls straight through when only we moved', () => {
    expect(pullNeedsChoice({ upstream: up, ahead: 3, behind: 0 })).toBe(false)
  })

  it('does nothing special when the two already match', () => {
    expect(pullNeedsChoice({ upstream: up, ahead: 0, behind: 0 })).toBe(false)
  })

  it('never routes a branch with no upstream to the modal', () => {
    // The modal pairs the branch against its upstream ref; without one there
    // is no pair to resolve and it would open empty.
    expect(pullNeedsChoice({ upstream: null, ahead: 2, behind: 2 })).toBe(false)
    expect(pullNeedsChoice({ ahead: 2, behind: 2 })).toBe(false)
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

describe('a drop between two local branches', () => {
  // GITWYRM-FRONTEND-13: the button offered to "Send 22 changes up" and the
  // result then said "Caught v1 up to main". No cloud is involved either way.
  const names = { source: 'v1', target: 'main' }

  it('says which branch catches up, matching the toast that follows', () => {
    const c = modeCopy('send', { ours: 22, theirs: 0 }, names)
    expect(c.action).toBe('Catch v1 up')
    expect(c.label).toBe('Catch up')
  })

  it('names the other branch when the drop goes the other way', () => {
    const c = modeCopy('get', { ours: 0, theirs: 22 }, names)
    expect(c.action).toBe('Catch main up')
  })

  it('never mentions the cloud or sending for a local pair', () => {
    for (const mode of ['get', 'send'] as const) {
      const c = modeCopy(mode, { ours: 3, theirs: 3 }, names)
      const all = `${c.action} ${c.caption} ${c.note.text}`
      expect(all).not.toMatch(/cloud/i)
      expect(all).not.toMatch(/\bsent?\b|\bup to date\b/i)
    }
  })

  it('leaves the cloud wording alone when no names are given', () => {
    expect(modeCopy('send', { ours: 22, theirs: 0 }).action).toBe('Send 22 changes up')
    expect(modeCopy('get', { ours: 0, theirs: 1 }).action).toBe('Get 1 change')
  })
})

describe('shownModes', () => {
  // origin/master into a checked-out master that tracks something else.
  const remoteIntoHead = { kind: 'branches' as const, canReset: true, sourceIsRemote: true }

  /**
   * Replace force-pushes the checked-out branch to ITS upstream. When the pair
   * on screen is not that upstream, it would overwrite a server branch the
   * user never saw.
   */
  it('never offers replace for a pair that is not a branch and its upstream', () => {
    expect(shownModes({ ours: 1, theirs: 23 }, remoteIntoHead)).toEqual(['blend', 'stack', 'reset'])
  })

  it('still offers replace between a branch and its own upstream', () => {
    const tracking = { kind: 'tracking' as const, canReset: false, sourceIsRemote: true }
    expect(shownModes({ ours: 1, theirs: 23 }, tracking)).toEqual(['replace', 'blend', 'stack'])
  })

  /** "Discard my commit and match the server" when the server has nothing new. */
  it('offers reset when only our side moved', () => {
    expect(shownModes({ ours: 1, theirs: 0 }, remoteIntoHead)).toEqual(['reset'])
  })

  it('does not offer reset when the receiving branch is not checked out', () => {
    expect(shownModes({ ours: 1, theirs: 2 }, { ...remoteIntoHead, canReset: false })).toEqual(['blend', 'stack'])
  })
})

describe('initialMode', () => {
  /** "Make master match this" in the menu must open on Reset, not fall back to another option. */
  it('honours the option a menu asked for', () => {
    expect(initialMode(['blend', 'stack', 'reset'], 'reset')).toBe('reset')
  })

  it('starts on a safe option when nothing was asked for', () => {
    expect(initialMode(['blend', 'stack', 'reset'], null)).toBe('stack')
    expect(initialMode(['replace', 'blend', 'stack'], null)).toBe('stack')
  })

  it('ignores a request for an option that is not on offer', () => {
    expect(initialMode(['blend', 'stack'], 'reset')).toBe('stack')
  })
})

describe('modeCopy reset', () => {
  it('names both branches for a local pair', () => {
    expect(modeCopy('reset', { ours: 1, theirs: 0 }, { source: 'origin/master', target: 'master' }).action).toBe(
      'Make master match origin/master',
    )
  })
})
