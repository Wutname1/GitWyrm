import { describe, expect, it } from 'vitest'
import type { RefInfo } from '@/lib/bindings'
import { syncedPair } from './refStack'

const head = (name: string): RefInfo => ({ name, type: 'head' })
const branch = (name: string): RefInfo => ({ name, type: 'branch' })
const remote = (name: string): RefInfo => ({ name, type: 'remote' })
const tag = (name: string): RefInfo => ({ name, type: 'tag' })

describe('syncedPair', () => {
  it('collapses a branch and its remote', () => {
    const pair = syncedPair([head('main'), remote('origin/main')])
    expect(pair?.local.name).toBe('main')
    expect(pair?.remote.name).toBe('origin/main')
    expect(pair?.rest).toEqual([])
  })

  it('still collapses when a tag shares the commit', () => {
    // The reported case: a release commit carries main, origin/main and a tag.
    // Folding all three into a "+2" chip hid the branch name behind a popover.
    const pair = syncedPair([head('main'), remote('origin/main'), tag('v1.2.3')])
    expect(pair?.local.name).toBe('main')
    expect(pair?.rest.map((r) => r.name)).toEqual(['v1.2.3'])
  })

  it('carries two tags alongside the pair', () => {
    const pair = syncedPair([
      branch('main'),
      remote('origin/main'),
      tag('v1.2.3'),
      tag('latest'),
    ])
    expect(pair?.rest.map((r) => r.name)).toEqual(['v1.2.3', 'latest'])
  })

  it('hands a tag pile back to the stack', () => {
    // Three tags plus a pair is five chips in a 138px column; the popover is
    // the better answer past a couple.
    expect(
      syncedPair([
        head('main'),
        remote('origin/main'),
        tag('v1.2.3'),
        tag('latest'),
        tag('stable'),
      ])
    ).toBeNull()
  })

  it('does not collapse a second branch', () => {
    // A real choice between two branches is what the stack exists for.
    expect(syncedPair([head('main'), branch('feature'), remote('origin/main')])).toBeNull()
  })

  it('does not collapse two remotes', () => {
    expect(
      syncedPair([head('main'), remote('origin/main'), remote('upstream/main')])
    ).toBeNull()
  })

  it('does not collapse branches that merely share a commit', () => {
    // origin/release is not main's counterpart, so hiding it would be a lie.
    expect(syncedPair([head('main'), remote('origin/release')])).toBeNull()
  })

  it('does not collapse a lone branch or a lone tag', () => {
    expect(syncedPair([head('main')])).toBeNull()
    expect(syncedPair([head('main'), tag('v1.2.3')])).toBeNull()
  })

  it('does not collapse a remote with no local counterpart', () => {
    expect(syncedPair([remote('origin/main'), remote('origin/dev')])).toBeNull()
  })

  it('matches a remote whose name contains slashes', () => {
    const pair = syncedPair([branch('feat/thing'), remote('origin/feat/thing')])
    expect(pair?.local.name).toBe('feat/thing')
  })
})
