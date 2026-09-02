import { describe, expect, it } from 'vitest'
import { parseRepoUrlQuery } from './repoUrlSearch'

describe('parseRepoUrlQuery', () => {
  it('reads a github address with or without the .git suffix', () => {
    for (const url of [
      'https://github.com/Wutname1/gitwyrm',
      'https://github.com/Wutname1/gitwyrm.git',
      'https://github.com/Wutname1/gitwyrm/',
    ]) {
      expect(parseRepoUrlQuery(url)).toMatchObject({
        host: 'github.com',
        slug: 'Wutname1/gitwyrm',
      })
    }
  })

  it('reads ssh and scp-like addresses', () => {
    expect(parseRepoUrlQuery('git@github.com:Wutname1/gitwyrm.git')).toMatchObject({
      host: 'github.com',
      slug: 'Wutname1/gitwyrm',
    })
    expect(
      parseRepoUrlQuery('ssh://git@github.com:22/Wutname1/gitwyrm.git'),
    ).toMatchObject({ host: 'github.com', slug: 'Wutname1/gitwyrm' })
  })

  it('keeps the address as typed so the lookup and the clone box agree', () => {
    expect(parseRepoUrlQuery('  https://github.com/o/r.git  ')?.url).toBe(
      'https://github.com/o/r.git',
    )
  })

  it('drops a query string or fragment picked up from a browser', () => {
    expect(parseRepoUrlQuery('https://github.com/o/r?tab=readme#top')).toMatchObject({
      slug: 'o/r',
      url: 'https://github.com/o/r',
    })
  })

  it('has no slug for a deeper path such as a gitlab subgroup', () => {
    expect(parseRepoUrlQuery('https://gitlab.com/group/sub/repo.git')).toMatchObject({
      host: 'gitlab.com',
      slug: null,
    })
  })

  it('treats ordinary search text as a search', () => {
    for (const text of [
      'gitwyrm',
      'my repo',
      'feature/thing',
      'C:\\code\\GitWyrm',
      '/home/me/code',
      './relative',
      'ftp://github.com/o/r',
      'https://github.com',
    ]) {
      expect(parseRepoUrlQuery(text)).toBeNull()
    }
  })
})
