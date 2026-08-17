import { describe, expect, it } from 'vitest'
import { normalizeRemoteUrl, remoteNameError, suggestRemoteName } from './remoteAdd'

describe('normalizeRemoteUrl', () => {
  it('accepts a repository page copied out of the address bar', () => {
    expect(normalizeRemoteUrl('https://github.com/comcast-mgee/MGEEAnalytics')).toBe(
      'https://github.com/comcast-mgee/MGEEAnalytics.git'
    )
    expect(normalizeRemoteUrl('  https://github.com/comcast-mgee/MGEEAnalytics/  ')).toBe(
      'https://github.com/comcast-mgee/MGEEAnalytics.git'
    )
  })

  it('leaves an address that is already a clone URL alone', () => {
    expect(normalizeRemoteUrl('https://github.com/owner/repo.git')).toBe(
      'https://github.com/owner/repo.git'
    )
    expect(normalizeRemoteUrl('git@github.com:owner/repo.git')).toBe('git@github.com:owner/repo.git')
    expect(normalizeRemoteUrl('ssh://git@github.com/owner/repo.git')).toBe(
      'ssh://git@github.com/owner/repo.git'
    )
  })

  it('cuts a page inside the host UI back to the repository', () => {
    expect(normalizeRemoteUrl('https://github.com/owner/repo/tree/feature/thing')).toBe(
      'https://github.com/owner/repo.git'
    )
    expect(normalizeRemoteUrl('https://github.com/owner/repo/pull/12/files')).toBe(
      'https://github.com/owner/repo.git'
    )
    expect(normalizeRemoteUrl('https://gitlab.com/group/sub/repo/-/tree/main')).toBe(
      'https://gitlab.com/group/sub/repo.git'
    )
    expect(normalizeRemoteUrl('https://bitbucket.org/team/repo/src/main/README.md')).toBe(
      'https://bitbucket.org/team/repo.git'
    )
  })

  it('drops the query string and fragment a shared link carries', () => {
    expect(normalizeRemoteUrl('https://github.com/owner/repo?tab=readme-ov-file#install')).toBe(
      'https://github.com/owner/repo.git'
    )
  })

  it('fills in a missing https:// on a bare host paste', () => {
    expect(normalizeRemoteUrl('github.com/owner/repo')).toBe('https://github.com/owner/repo.git')
  })

  it('unwraps a copied clone command and surrounding punctuation', () => {
    expect(normalizeRemoteUrl('git clone https://github.com/owner/repo.git')).toBe(
      'https://github.com/owner/repo.git'
    )
    expect(normalizeRemoteUrl('git clone --depth 1 https://github.com/owner/repo')).toBe(
      'https://github.com/owner/repo.git'
    )
    expect(normalizeRemoteUrl('"https://github.com/owner/repo"')).toBe(
      'https://github.com/owner/repo.git'
    )
  })

  it('adds the .git suffix on ssh shapes too', () => {
    expect(normalizeRemoteUrl('git@github.com:owner/repo')).toBe('git@github.com:owner/repo.git')
    expect(normalizeRemoteUrl('ssh://git@gitlab.com/group/repo')).toBe(
      'ssh://git@gitlab.com/group/repo.git'
    )
  })

  it('keeps deep paths and ports on hosts it does not recognize', () => {
    expect(normalizeRemoteUrl('https://git.example.com/a/b/c/d')).toBe(
      'https://git.example.com/a/b/c/d'
    )
    expect(normalizeRemoteUrl('ssh://git@git.example.com:2222/a/b.git')).toBe(
      'ssh://git@git.example.com:2222/a/b.git'
    )
  })

  it('keeps the Azure DevOps path shape, which has no .git suffix', () => {
    expect(normalizeRemoteUrl('https://dev.azure.com/org/project/_git/repo')).toBe(
      'https://dev.azure.com/org/project/_git/repo'
    )
    expect(
      normalizeRemoteUrl('https://dev.azure.com/org/project/_git/repo?path=/src&version=GBmain')
    ).toBe('https://dev.azure.com/org/project/_git/repo')
  })

  it('handles GitHub Enterprise the way it handles github.com', () => {
    expect(normalizeRemoteUrl('https://github.myco.com/owner/repo/tree/main')).toBe(
      'https://github.myco.com/owner/repo.git'
    )
  })

  it('leaves local paths and half-typed text untouched', () => {
    for (const value of ['C:\\src\\repo', '/srv/git/repo.git', './sibling', '\\\\server\\share']) {
      expect(normalizeRemoteUrl(value), value).toBe(value)
    }
    expect(normalizeRemoteUrl('https://')).toBe('https://')
    expect(normalizeRemoteUrl('   ')).toBe('')
  })

  it('waits for a whole address before rewriting one being typed', () => {
    expect(normalizeRemoteUrl('https://github.com/owner')).toBe('https://github.com/owner')
    expect(normalizeRemoteUrl('https://gitlab.com/group')).toBe('https://gitlab.com/group')
  })
})

describe('suggestRemoteName', () => {
  it('uses origin when nothing is using it', () => {
    expect(suggestRemoteName('https://github.com/owner/repo.git', [])).toBe('origin')
  })

  it('falls back to the owner from the address when origin is taken', () => {
    expect(
      suggestRemoteName('https://github.com/comcast-mgee/MGEEAnalytics', ['origin'])
    ).toBe('comcast-mgee')
    expect(suggestRemoteName('git@github.com:Some.Owner/repo.git', ['origin'])).toBe('some.owner')
  })

  it('falls back to upstream, then a numbered origin', () => {
    expect(suggestRemoteName('https://example.com', ['origin'])).toBe('upstream')
    expect(suggestRemoteName('https://github.com/owner/repo', ['origin', 'owner'])).toBe('upstream')
    expect(suggestRemoteName('https://github.com/owner/repo', ['origin', 'owner', 'upstream'])).toBe(
      'origin-2'
    )
    expect(
      suggestRemoteName('https://github.com/owner/repo', ['origin', 'owner', 'upstream', 'origin-2'])
    ).toBe('origin-3')
  })

  it('skips a Bitbucket Server /scm/ prefix, which names nothing', () => {
    expect(suggestRemoteName('https://git.myco.com/scm/proj/repo.git', ['origin'])).toBe('proj')
  })

  it('always returns a usable name, whatever the address is', () => {
    expect(suggestRemoteName('', [])).toBe('origin')
    expect(suggestRemoteName('   ', ['origin'])).toBe('upstream')
    expect(suggestRemoteName('C:\\src\\repo', ['origin'])).toBe('upstream')
  })
})

describe('remoteNameError', () => {
  it('accepts a blank name, which means "use the suggestion"', () => {
    expect(remoteNameError('', ['origin'])).toBeNull()
    expect(remoteNameError('   ', ['origin'])).toBeNull()
  })

  it('accepts ordinary names', () => {
    expect(remoteNameError('origin', [])).toBeNull()
    expect(remoteNameError('my_fork-2.0', ['origin'])).toBeNull()
  })

  it('explains the ones git would reject', () => {
    expect(remoteNameError('origin', ['origin'])).toBe('That name is already used.')
    expect(remoteNameError('my remote', [])).toBe("Names can't have spaces.")
    expect(remoteNameError('team/fork', [])).toBe(
      'Use letters, numbers, dashes, dots, or underscores.'
    )
    expect(remoteNameError('-fork', [])).toBe("Names can't start with a dash or dot.")
  })
})
