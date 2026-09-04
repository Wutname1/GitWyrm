import { describe, expect, it } from 'vitest'
import type { BranchInfo, RemoteInfo } from '@/lib/bindings'
import { publishedBranchTarget } from './branchDelete'

const local = (name: string, upstream: string | null): BranchInfo => ({
  name,
  is_head: false,
  upstream,
  ahead: 0,
  behind: 0,
  sync: { kind: 'in_sync' },
  time: 100,
  tip: 'abc1234',
})

const remote = (name: string, branches: string[]): RemoteInfo =>
  ({
    name,
    branches: branches.map((branch) => ({ name: branch })),
  }) as RemoteInfo

describe('publishedBranchTarget', () => {
  it('finds the exact remote branch configured as the upstream', () => {
    expect(
      publishedBranchTarget(
        local('feature/local-name', 'origin/review/remote-name'),
        [remote('origin', ['review/remote-name'])],
      ),
    ).toEqual({ remote: 'origin', branch: 'review/remote-name' })
  })

  it('does not offer a same-named branch from an unrelated remote', () => {
    expect(
      publishedBranchTarget(
        local('feature', 'upstream/feature'),
        [remote('origin', ['feature']), remote('upstream', [])],
      ),
    ).toBeNull()
  })

  it('supports remote names that contain a slash', () => {
    expect(
      publishedBranchTarget(
        local('feature', 'team/fork/feature'),
        [remote('team', []), remote('team/fork', ['feature'])],
      ),
    ).toEqual({ remote: 'team/fork', branch: 'feature' })
  })
})
