import { describe, expect, it } from 'vitest'
import type { SubmoduleStatus } from '@/lib/bindings'
import { describeHead, isDetached, readSubmodule } from './submoduleState'

const sub = (over: Partial<SubmoduleStatus> = {}): SubmoduleStatus =>
  ({
    path: 'vendor/lib',
    name: 'lib',
    url: 'https://example.com/lib.git',
    branch: 'main',
    recorded_sha: 'aaaaaaa',
    workdir_sha: 'aaaaaaa',
    ahead: 0,
    behind: 0,
    head_branch: null,
    state: 'in_sync',
    ...over,
  }) as SubmoduleStatus

describe('readSubmodule', () => {
  it('treats catching up after someone else moved it as routine, not destructive', () => {
    // The reported case. Calling this "Undo my changes" blamed the user for a
    // state a pull had left, and dressed the only fix as something dangerous.
    const r = readSubmodule(sub({ state: 'moved', behind: 3, ahead: 0 }))
    expect(r.situation).toBe('behind-project')
    expect(r.matchIsSafe).toBe(true)
    expect(r.canMatchProject).toBe(true)
    expect(r.meaning).toMatch(/nothing of yours/i)
  })

  it('warns when matching the project would drop work only in this folder', () => {
    const ahead = readSubmodule(sub({ state: 'moved', ahead: 2, behind: 0 }))
    expect(ahead.situation).toBe('ahead-of-project')
    expect(ahead.matchIsSafe).toBe(false)

    const both = readSubmodule(sub({ state: 'moved', ahead: 2, behind: 3 }))
    expect(both.situation).toBe('diverged')
    expect(both.matchIsSafe).toBe(false)
  })

  it('offers nothing to fix when it already matches', () => {
    const r = readSubmodule(sub({ state: 'in_sync' }))
    expect(r.situation).toBe('matches')
    expect(r.canMatchProject).toBe(false)
    expect(r.label).toBe('')
  })

  it('treats a folder that was never downloaded as safe to fill in', () => {
    const r = readSubmodule(sub({ state: 'uninitialized', workdir_sha: null }))
    expect(r.situation).toBe('not-downloaded')
    expect(r.matchIsSafe).toBe(true)
  })

  it('never uses git vocabulary in what the user reads', () => {
    const cases: SubmoduleStatus[] = [
      sub({ state: 'uninitialized', workdir_sha: null }),
      sub({ state: 'in_sync' }),
      sub({ state: 'moved', behind: 2 }),
      sub({ state: 'moved', ahead: 2 }),
      sub({ state: 'moved', ahead: 1, behind: 1 }),
    ]
    for (const c of cases) {
      const { meaning, label } = readSubmodule(c)
      for (const word of [/submodule/i, /detached/i, /HEAD/, /gitlink/i, /checkout/i]) {
        expect(meaning).not.toMatch(word)
        expect(label).not.toMatch(word)
      }
    }
  })
})

describe('detached state', () => {
  it('is reported for a downloaded folder that is not on a branch', () => {
    // What `git submodule update` leaves behind: normal, but a commit made
    // there belongs to no branch and is easy to lose.
    expect(isDetached(sub({ state: 'in_sync', head_branch: null }))).toBe(true)
    expect(describeHead(sub({ state: 'in_sync', head_branch: null }))).toBe('Not on a branch')
  })

  it('is not reported when it is on a branch', () => {
    expect(isDetached(sub({ state: 'in_sync', head_branch: 'main' }))).toBe(false)
    expect(describeHead(sub({ state: 'in_sync', head_branch: 'main' }))).toBe('On main')
  })

  it('is not claimed for a folder that was never downloaded', () => {
    // There is no checkout at all, so "not on a branch" would be misleading.
    const empty = sub({ state: 'uninitialized', workdir_sha: null, head_branch: null })
    expect(isDetached(empty)).toBe(false)
    expect(describeHead(empty)).toBe('Not downloaded')
  })
})
