import { describe, expect, it } from 'vitest'
import type { RefInfo } from '@/lib/bindings'
import { groupRefs, syncedPair } from './refStack'

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

describe('groupRefs', () => {
  it('collapses two synced branches into two chips, not four rows', () => {
    // The reported case: a commit at the tip of both `edge` and `main`, each in
    // sync with its remote, listed all four refs in a popover. There is no
    // choice to make between a branch and its own remote-tracking ref.
    const { groups, tags } = groupRefs([
      head('edge'),
      remote('origin/edge'),
      branch('main'),
      remote('origin/main'),
    ])
    expect(groups).toHaveLength(2)
    expect(groups.map((g) => g.primary.name)).toEqual(['edge', 'main'])
    expect(groups.map((g) => g.syncedWith?.name)).toEqual(['origin/edge', 'origin/main'])
    expect(tags).toEqual([])
  })

  it('pairs on the configured upstream, not the name', () => {
    // A local renamed away from its remote is still a real pair; a name match
    // alone would miss it and show both refs.
    const { groups } = groupRefs([branch('feature'), remote('origin/old-name')], (name) =>
      name === 'feature' ? 'origin/old-name' : null,
    )
    expect(groups).toHaveLength(1)
    expect(groups[0].syncedWith?.name).toBe('origin/old-name')
  })

  it('keeps a branch separate from a same-named remote it does not track', () => {
    // Sharing a commit is not a tracking relationship, so folding them would
    // claim something untrue.
    const { groups } = groupRefs([branch('main'), remote('origin/main')], () => 'origin/other')
    expect(groups).toHaveLength(2)
    expect(groups[0].syncedWith).toBeNull()
  })

  it('leaves two remotes offering the same branch as a real choice', () => {
    const { groups } = groupRefs([head('main'), remote('origin/main'), remote('upstream/main')])
    expect(groups).toHaveLength(3)
    expect(groups[0].syncedWith).toBeNull()
  })

  it('gives a remote with no local counterpart its own chip', () => {
    const { groups } = groupRefs([remote('origin/main'), remote('origin/dev')])
    expect(groups.map((g) => g.primary.name)).toEqual(['origin/main', 'origin/dev'])
  })

  it('separates tags from branch chips', () => {
    const { groups, tags } = groupRefs([head('main'), remote('origin/main'), tag('v1.2.3')])
    expect(groups).toHaveLength(1)
    expect(tags.map((t) => t.name)).toEqual(['v1.2.3'])
  })
})

/**
 * The popover lists one row per branch, built from the same groups as the
 * chips. Two rows both reading `main` was the reported noise -- a branch and
 * its own remote copy are one branch, not two.
 */
describe('popover rows', () => {
  const rowsFor = (refs: RefInfo[], upstreamOf?: (n: string) => string | null) => {
    const { groups, tags } = groupRefs(refs, upstreamOf)
    return [
      ...groups.map((g) => ({ name: g.primary.name, synced: g.syncedWith?.name ?? null })),
      ...tags.map((t) => ({ name: t.name, synced: null })),
    ]
  }

  it('gives one row per branch, not one per ref', () => {
    const rows = rowsFor([
      { name: 'edge', type: 'head' },
      { name: 'origin/edge', type: 'remote' },
      { name: 'main', type: 'branch' },
      { name: 'origin/main', type: 'remote' },
    ])
    expect(rows).toEqual([
      { name: 'edge', synced: 'origin/edge' },
      { name: 'main', synced: 'origin/main' },
    ])
  })

  it('still lists a tag as its own row', () => {
    const rows = rowsFor([head('main'), remote('origin/main'), tag('v1.2.3')])
    expect(rows).toEqual([
      { name: 'main', synced: 'origin/main' },
      { name: 'v1.2.3', synced: null },
    ])
  })

  it('keeps a genuine choice as separate rows', () => {
    // Two servers offering the same branch is a real decision, so both show.
    const rows = rowsFor([head('main'), remote('origin/main'), remote('upstream/main')])
    expect(rows).toHaveLength(3)
  })
})
