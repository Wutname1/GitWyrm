import { describe, expect, it } from 'vitest'
import { groupHostingProviders } from './hostingGroups'
import type { HostProviderInfo } from '@/lib/bindings'

function host(over: Partial<HostProviderInfo>): HostProviderInfo {
  return {
    id: 'github',
    display_name: 'GitHub',
    auth_kind: 'device_code',
    token_url: null,
    required_scopes: [],
    connected_as: null,
    auth_error: null,
    capabilities: { pull_requests: true, issues: true, checks: true },
    ...over,
  } as HostProviderInfo
}

describe('groupHostingProviders', () => {
  it('puts a working sign-in in connected', () => {
    const g = groupHostingProviders([host({ connected_as: 'octocat' })])
    expect(g.connected).toHaveLength(1)
    expect(g.failing).toHaveLength(0)
    expect(g.available).toHaveLength(0)
  })

  it('puts a host with nothing saved in available', () => {
    const g = groupHostingProviders([host({})])
    expect(g.available).toHaveLength(1)
    expect(g.failing).toHaveLength(0)
  })

  it('separates a stale sign-in from one that was never set up', () => {
    // The bug this split exists for: both used to look like "not connected",
    // so a token that expired offered "add an integration" for a host that was
    // already configured.
    const g = groupHostingProviders([
      host({ id: 'github', auth_error: 'Bad credentials' }),
      host({ id: 'gitlab', display_name: 'GitLab' }),
    ])
    expect(g.failing.map((p) => p.id)).toEqual(['github'])
    expect(g.available.map((p) => p.id)).toEqual(['gitlab'])
  })

  it('keeps a connected host connected even if a check also errored', () => {
    // It has a working account name. Demoting it would take away Disconnect
    // over what may be one failed request.
    const g = groupHostingProviders([
      host({ connected_as: 'octocat', auth_error: 'rate limited' }),
    ])
    expect(g.connected).toHaveLength(1)
    expect(g.failing).toHaveLength(0)
  })

  it('handles the list not having loaded yet', () => {
    const g = groupHostingProviders(undefined)
    expect(g.connected).toEqual([])
    expect(g.failing).toEqual([])
    expect(g.available).toEqual([])
  })

  it('never puts one host in two groups', () => {
    const providers = [
      host({ id: 'github', connected_as: 'a' }),
      host({ id: 'gitlab', auth_error: 'nope' }),
      host({ id: 'bitbucket' }),
    ]
    const g = groupHostingProviders(providers)
    const total = g.connected.length + g.failing.length + g.available.length
    expect(total).toBe(providers.length)
  })
})
