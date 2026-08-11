import { describe, expect, it } from 'vitest'
import type { BranchInfo, BranchList } from '@/lib/bindings'
import { resolveDropPair } from './refSync'

const branch = (name: string, extra: Partial<BranchInfo> = {}): BranchInfo => ({
  name,
  is_head: false,
  upstream: null,
  ahead: 0,
  behind: 0,
  sync: { kind: 'never_pushed' },
  time: null,
  tip: null,
  ...extra,
})

const branches: BranchList = {
  local: [
    branch('main', { is_head: true, upstream: 'origin/main', sync: { kind: 'in_sync' } }),
    branch('feature/pr'),
  ],
  remote: ['origin/main', 'origin/feature/pr'],
}

describe('resolveDropPair direction', () => {
  // The whole point of the pair is WHICH branch moves. Getting it backwards
  // merges the wrong way round, and nothing downstream would catch it.
  it('makes the dragged branch the one that moves, when both are local', () => {
    const pair = resolveDropPair(
      { name: 'main', type: 'head' },
      { name: 'feature/pr', type: 'branch' },
      branches
    )

    expect(pair).toEqual({
      kind: 'branches',
      source: { name: 'feature/pr', type: 'branch' },
      target: { name: 'main', type: 'head' },
    })
  })

  it('makes the local branch the one that moves, when a remote ref is dragged', () => {
    const pair = resolveDropPair(
      { name: 'origin/feature/pr', type: 'remote' },
      { name: 'main', type: 'head' },
      branches
    )

    expect(pair).toEqual({
      kind: 'branches',
      source: { name: 'origin/feature/pr', type: 'remote' },
      target: { name: 'main', type: 'head' },
    })
  })

  it('still moves the local branch when it is dragged onto an unrelated remote ref', () => {
    const pair = resolveDropPair(
      { name: 'main', type: 'head' },
      { name: 'origin/feature/pr', type: 'remote' },
      branches
    )

    expect(pair).toEqual({
      kind: 'branches',
      source: { name: 'origin/feature/pr', type: 'remote' },
      target: { name: 'main', type: 'head' },
    })
  })

  it('pairs a branch with its own upstream as a tracking pair, not a branch pair', () => {
    const pair = resolveDropPair(
      { name: 'origin/main', type: 'remote' },
      { name: 'main', type: 'head' },
      branches
    )

    expect(pair?.kind).toBe('tracking')
  })

  it('refuses tags, self-drops and two remote refs', () => {
    const tag = resolveDropPair(
      { name: 'v1.0', type: 'tag' },
      { name: 'main', type: 'head' },
      branches
    )
    expect(tag).toBeNull()

    const self = resolveDropPair(
      { name: 'main', type: 'head' },
      { name: 'main', type: 'head' },
      branches
    )
    expect(self).toBeNull()

    const twoRemotes = resolveDropPair(
      { name: 'origin/main', type: 'remote' },
      { name: 'origin/feature/pr', type: 'remote' },
      branches
    )
    expect(twoRemotes).toBeNull()
  })
})
