import { describe, expect, it } from 'vitest'
import { remoteUrlKind, swapRemoteUrl } from './remoteUrlSwap'

describe('remoteUrlKind', () => {
  it('identifies both shapes for the major hosts', () => {
    expect(remoteUrlKind('https://github.com/owner/repo.git')).toBe('https')
    expect(remoteUrlKind('git@github.com:owner/repo.git')).toBe('ssh')
    expect(remoteUrlKind('ssh://git@gitlab.com/group/repo.git')).toBe('ssh')
    expect(remoteUrlKind('https://gitlab.com/group/subgroup/repo.git')).toBe('https')
  })

  it('returns null for anything that is not a swappable remote URL', () => {
    for (const url of [
      'C:\\src\\repo',
      'c:/src/repo',
      '/srv/git/repo.git',
      './relative',
      'file:///srv/repo',
      'ftp://example.com/o/r',
      'https://github.com/',
      '',
      '   ',
    ]) {
      expect(remoteUrlKind(url), url).toBeNull()
      expect(swapRemoteUrl(url), url).toBeNull()
    }
  })
})

describe('swapRemoteUrl', () => {
  it('converts https to the scp-like ssh form', () => {
    expect(swapRemoteUrl('https://github.com/owner/repo.git')).toBe('git@github.com:owner/repo.git')
    // A missing .git suffix is added back on the way out.
    expect(swapRemoteUrl('https://github.com/owner/repo')).toBe('git@github.com:owner/repo.git')
  })

  it('converts both ssh shapes to https', () => {
    expect(swapRemoteUrl('git@github.com:owner/repo.git')).toBe('https://github.com/owner/repo.git')
    expect(swapRemoteUrl('ssh://git@github.com/owner/repo.git')).toBe(
      'https://github.com/owner/repo.git'
    )
  })

  it('round-trips back to the original URL', () => {
    for (const url of [
      'https://github.com/owner/repo.git',
      'git@github.com:owner/repo.git',
      'https://gitlab.com/group/subgroup/repo.git',
      'git@gitlab.com:group/subgroup/repo.git',
      'https://bitbucket.org/owner/repo.git',
    ]) {
      const swapped = swapRemoteUrl(url)
      expect(swapped, url).not.toBeNull()
      expect(swapRemoteUrl(swapped as string), url).toBe(url)
    }
  })

  it('keeps deep GitLab subgroup paths intact', () => {
    expect(swapRemoteUrl('https://gitlab.com/group/sub/deeper/repo.git')).toBe(
      'git@gitlab.com:group/sub/deeper/repo.git'
    )
  })

  it('keeps enterprise hosts and their subdomains', () => {
    expect(swapRemoteUrl('https://github.myco.com/o/r.git')).toBe('git@github.myco.com:o/r.git')
    expect(swapRemoteUrl('git@gitlab.internal.myco.com:o/r.git')).toBe(
      'https://gitlab.internal.myco.com/o/r.git'
    )
  })

  it('drops an https credential in favor of the git ssh user', () => {
    expect(swapRemoteUrl('https://someone@github.com/o/r.git')).toBe('git@github.com:o/r.git')
  })

  it('uses the ssh:// shape when a port must be preserved', () => {
    // scp-like syntax cannot carry a port, so the swap keeps the scheme form.
    expect(swapRemoteUrl('https://git.example.com:2222/o/r.git')).toBe(
      'ssh://git@git.example.com:2222/o/r.git'
    )
    expect(swapRemoteUrl('ssh://git@git.example.com:2222/o/r.git')).toBe(
      'https://git.example.com/o/r.git'
    )
  })

  it('removes only one .git suffix', () => {
    expect(swapRemoteUrl('https://github.com/o/my.git.git')).toBe('git@github.com:o/my.git.git')
  })

  it('tolerates surrounding whitespace and trailing slashes', () => {
    expect(swapRemoteUrl('  https://github.com/owner/repo.git/  ')).toBe(
      'git@github.com:owner/repo.git'
    )
  })
})
