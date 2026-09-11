import { describe, expect, it } from 'vitest'
import { buildStarters } from './agentDeskStarters'

/**
 * The whole value of this row is that every entry names work GitWyrm has
 * really measured. A suggestion that could have been written without opening
 * the repository is the generic version of the idea and costs the row its
 * meaning, so the interesting cases here are all about staying silent.
 */
describe('buildStarters', () => {
  it('says nothing about a repository with nothing in it', () => {
    expect(buildStarters({ uncommittedFileCount: 0, branchName: null, aheadCount: null })).toEqual([])
  })

  it('offers the uncommitted work, counted properly', () => {
    const one = buildStarters({ uncommittedFileCount: 1, branchName: null, aheadCount: null })
    expect(one[0].label).toBe('Review my 1 file')
    expect(one[0].label).not.toContain('(s)')

    const many = buildStarters({ uncommittedFileCount: 4, branchName: null, aheadCount: null })
    expect(many[0].label).toBe('Review my 4 files')
  })

  it('never suggests anything about commits it could not count', () => {
    // `null` is unknown, not zero. A branch whose unpushed count failed to
    // load must not produce "Explain the 0 commits on main".
    const unknown = buildStarters({ uncommittedFileCount: 0, branchName: 'main', aheadCount: null })
    expect(unknown).toEqual([])

    const none = buildStarters({ uncommittedFileCount: 0, branchName: 'main', aheadCount: 0 })
    expect(none).toEqual([])
  })

  it('offers the branch only when there is something on it to explain', () => {
    const ahead = buildStarters({ uncommittedFileCount: 0, branchName: 'feature/x', aheadCount: 2 })
    expect(ahead).toHaveLength(1)
    expect(ahead[0].label).toBe('Explain the 2 commits on feature/x')

    const single = buildStarters({ uncommittedFileCount: 0, branchName: 'main', aheadCount: 1 })
    expect(single[0].label).toBe('Explain the 1 commit on main')
  })

  it('gives every starter a prompt that can stand on its own', () => {
    const all = buildStarters({ uncommittedFileCount: 3, branchName: 'main', aheadCount: 2 })
    expect(all.length).toBeGreaterThan(1)
    for (const starter of all) {
      expect(starter.prompt.trim().length).toBeGreaterThan(20)
      expect(starter.id.trim()).toBeTruthy()
    }
    // Ids are used as React keys.
    expect(new Set(all.map((s) => s.id)).size).toBe(all.length)
  })
})
