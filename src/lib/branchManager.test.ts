import { describe, expect, it } from 'vitest'
import type { BranchInfo, RemoteInfo, SyncState } from '@/lib/bindings'
import {
  buildBranchRows,
  deleteTargets,
  locationsOf,
  matchesQuery,
  rowCapabilities,
  riskyLocations,
  riskyRows,
  sortRows,
} from './branchManager'

const local = (
  name: string,
  sync: SyncState,
  extra: Partial<BranchInfo> = {},
): BranchInfo => ({
  name,
  is_head: false,
  upstream: null,
  ahead: 0,
  behind: 0,
  sync,
  time: 100,
  tip: 'abc1234',
  ...extra,
})

const inSync: SyncState = { kind: 'in_sync' }
const remoteWith = (name: string, branches: string[], opts: Record<string, unknown> = {}): RemoteInfo =>
  ({
    name,
    url: `https://example.com/${name}.git`,
    push_url: null,
    branches: branches.map((b) => ({
      name: b,
      tip: 'abc1234',
      time: 100,
      summary: 'a commit',
      local_counterpart: null,
      tracked_by: null,
      ahead_of_local: 0,
      behind_local: 0,
      local_only_missing: false,
      ...opts,
    })),
    missing_locally: 0,
    provider: null,
    web_base: null,
  }) as unknown as RemoteInfo

describe('buildBranchRows', () => {
  it('joins a local branch and its remote copy into one row', () => {
    // The whole point of the manager: deciding whether a branch can go means
    // seeing both copies at once, which the split sidebar never showed.
    const rows = buildBranchRows(
      [local('main', inSync, { upstream: 'origin/main', is_head: true })],
      [remoteWith('origin', ['origin/main'], { tracked_by: 'main' })],
    )
    expect(rows).toHaveLength(1)
    expect(rows[0].name).toBe('main')
    expect(rows[0].local).not.toBeNull()
    expect(rows[0].remotes).toEqual(['origin'])
    expect(rows[0].isCurrent).toBe(true)
  })

  it('pairs on the tracked upstream rather than the name', () => {
    const rows = buildBranchRows(
      [local('feature', inSync, { upstream: 'origin/old-name' })],
      [remoteWith('origin', ['origin/old-name'], { tracked_by: 'feature' })],
    )
    expect(rows).toHaveLength(1)
    expect(rows[0].name).toBe('feature')
  })

  it('keeps a remote-only branch as its own row', () => {
    const rows = buildBranchRows([], [remoteWith('origin', ['origin/dev'])])
    expect(rows).toHaveLength(1)
    expect(rows[0].name).toBe('dev')
    expect(rows[0].local).toBeNull()
    expect(rows[0].remotes).toEqual(['origin'])
  })

  it('reads counts from sync, not the raw ahead/behind fields', () => {
    // BranchInfo.ahead/behind are (0,0) for every state but diverged, so a
    // branch with unpushed work looks identical to a synced one on those.
    const rows = buildBranchRows(
      [local('wip', { kind: 'diverged', ahead: 3, behind: 2 })],
      [],
    )
    expect(rows[0].ahead).toBe(3)
    expect(rows[0].behind).toBe(2)
  })
})

describe('rowCapabilities', () => {
  it('offers a pull only for a clean fast-forward', () => {
    const [behind] = buildBranchRows([local('a', { kind: 'diverged', ahead: 0, behind: 4 })], [])
    expect(rowCapabilities(behind).canPull).toBe(true)
  })

  it('refuses to pull a branch holding its own commits', () => {
    // That needs a real merge, which is not a thing to do to ten branches at
    // once without looking at any of them.
    const [both] = buildBranchRows([local('a', { kind: 'diverged', ahead: 2, behind: 4 })], [])
    expect(rowCapabilities(both).canPull).toBe(false)
  })

  it('flags a never-pushed branch as losing work', () => {
    const [fresh] = buildBranchRows([local('a', { kind: 'never_pushed' })], [])
    expect(rowCapabilities(fresh).losesWork).toBe(true)
  })

  it('flags unpushed commits as losing work', () => {
    const [ahead] = buildBranchRows([local('a', { kind: 'diverged', ahead: 2, behind: 0 })], [])
    expect(rowCapabilities(ahead).losesWork).toBe(true)
  })

  it('treats a fully synced branch as safe to delete', () => {
    const [synced] = buildBranchRows([local('a', inSync, { upstream: 'origin/a' })], [])
    expect(rowCapabilities(synced).losesWork).toBe(false)
  })

  it('collects the risky rows for the confirmation', () => {
    const rows = buildBranchRows(
      [local('safe', inSync), local('fresh', { kind: 'never_pushed' })],
      [],
    )
    expect(riskyRows(rows).map((r) => r.name)).toEqual(['fresh'])
  })
})

describe('sortRows', () => {
  it('always leads with the current branch', () => {
    // It cannot be deleted, and burying it reads as though it were missing.
    const rows = buildBranchRows(
      [local('zebra', inSync, { is_head: true }), local('alpha', inSync)],
      [],
    )
    expect(sortRows(rows, 'name')[0].name).toBe('zebra')
  })

  it('puts the oldest first when sorting by stale', () => {
    const rows = buildBranchRows(
      [local('new', inSync, { time: 900 }), local('old', inSync, { time: 100 })],
      [],
    )
    expect(sortRows(rows, 'stale').map((r) => r.name)).toEqual(['old', 'new'])
  })
})

describe('matchesQuery', () => {
  it('matches on name, case-insensitively', () => {
    const [row] = buildBranchRows([local('Feature/Login', inSync)], [])
    expect(matchesQuery(row, 'login')).toBe(true)
    expect(matchesQuery(row, 'nope')).toBe(false)
    expect(matchesQuery(row, '  ')).toBe(true)
  })
})

/**
 * A branch that lives in several places is several things a person may want to
 * delete, and they rarely mean all of them. One checkbox per row made it
 * impossible to drop a stale copy from one remote while keeping the rest.
 */
describe('per-location selection', () => {
  const twoRemotes = () =>
    buildBranchRows(
      [local('main', inSync, { upstream: 'origin/main' })],
      [remoteWith('origin', ['origin/main']), remoteWith('fork', ['fork/main'])],
    )

  it('records every remote a branch is on', () => {
    const [row] = twoRemotes()
    expect(row.remotes).toEqual(['origin', 'fork'])
  })

  it('offers one tickable copy per place it lives', () => {
    const [row] = twoRemotes()
    expect(locationsOf(row)).toEqual(['local', 'origin', 'fork'])
  })

  it('deletes only the copy that was ticked', () => {
    const [row] = twoRemotes()
    const targets = deleteTargets([{ name: 'main', where: 'fork', row }])
    expect(targets).toEqual([{ name: 'main', local: false, remote: 'fork' }])
  })

  it('splits a branch ticked on two remotes into one push each', () => {
    const [row] = twoRemotes()
    const targets = deleteTargets([
      { name: 'main', where: 'origin', row },
      { name: 'main', where: 'fork', row },
    ])
    expect(targets).toHaveLength(2)
    expect(targets.map((t) => t.remote)).toEqual(['origin', 'fork'])
  })

  it('deletes the local copy once, not once per remote', () => {
    const [row] = twoRemotes()
    const targets = deleteTargets([
      { name: 'main', where: 'local', row },
      { name: 'main', where: 'origin', row },
      { name: 'main', where: 'fork', row },
    ])
    expect(targets.filter((t) => t.local)).toHaveLength(1)
  })

  it('handles a local-only tick with no remote', () => {
    const [row] = buildBranchRows([local('wip', { kind: 'never_pushed' })], [])
    expect(deleteTargets([{ name: 'wip', where: 'local', row }])).toEqual([
      { name: 'wip', local: true, remote: null },
    ])
  })

  it('only warns about losing work when the local copy is ticked', () => {
    // Removing a server copy while keeping the one here loses nothing.
    const [row] = buildBranchRows([local('wip', { kind: 'never_pushed' })], [])
    expect(riskyLocations([{ name: 'wip', where: 'local', row }])).toHaveLength(1)
    expect(riskyLocations([{ name: 'wip', where: 'origin', row }])).toHaveLength(0)
  })
})
