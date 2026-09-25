import { describe, expect, it } from 'vitest'
import type { MehenPushNote } from './bindings'
import { allFixKeys, fixKey, isMehenStale, newFixes, pushNoteHint, pushNoteToast, unsafePackages } from './mehen'

const NOW = 1_800_000_000_000
const hoursAgo = (h: number) => NOW / 1000 - h * 3600

const note = (over: Partial<MehenPushNote> = {}): MehenPushNote => ({
  fixable: 2,
  checked_at: hoursAgo(3),
  seen_by_mehen: true,
  files: ['web/package.json', 'web/package-lock.json'],
  can_open: true,
  ...over,
})

describe('Mehen wording', () => {
  it('counts packages in plain words', () => {
    expect(unsafePackages(1)).toBe('1 package has a known security problem')
    expect(unsafePackages(3)).toBe('3 packages have known security problems')
  })

  it('treats an answer older than a week, or of unknown age, as out of date', () => {
    expect(isMehenStale(hoursAgo(24 * 6), NOW)).toBe(false)
    expect(isMehenStale(hoursAgo(24 * 8), NOW)).toBe(true)
    expect(isMehenStale(null, NOW)).toBe(true)
  })

  it('names the changed files by their file name', () => {
    expect(pushNoteHint(note(), NOW)).toBe(
      'These commits change package.json and package-lock.json. Mehen found 2 packages with known security problems here (checked 3h ago).',
    )
    expect(pushNoteHint(note({ files: ['a/Cargo.toml', 'b/Cargo.toml', 'Cargo.lock', 'go.mod'] }), NOW)).toContain(
      'change Cargo.toml, Cargo.lock and 1 more.',
    )
  })

  it('says when Mehen has not seen the changes being pushed', () => {
    expect(pushNoteHint(note({ seen_by_mehen: false }), NOW)).toMatch(/before these changes\.$/)
    expect(pushNoteToast(note({ seen_by_mehen: false })).description).toContain('Check again in Mehen')
    expect(pushNoteToast(note()).description).toBe('Mehen says 2 packages have known security problems in this project.')
  })
})

describe('new Mehen fixes', () => {
  const problem = (name: string, fixed_in = '2.0.0') => ({ name, ecosystem: 'npm', version: '1.0.0', fixed_in, severity: 'HIGH', summary: '', url: '' })
  const repo = (path: string, names: string[]) => ({ path, checked_at: 0, fixable: names.length, outdated: 0, problems: names.map((n) => problem(n)) })

  it('announces only fixes it has not seen, in open repositories', () => {
    const before = [repo('C:\\code\\a', ['old'])]
    const seen = new Set(allFixKeys(before))
    const after = [repo('C:\\code\\a', ['old', 'fresh']), repo('C:\\code\\closed', ['x'])]
    const found = newFixes(after, ['c:/code/a'], seen)
    expect(found.map((f) => [f.repo.path, f.count])).toEqual([['C:\\code\\a', 1]])
    expect(newFixes(after, ['C:\\code\\a'], new Set(allFixKeys(after)))).toEqual([])
  })

  it('treats a newer fix version for the same package as new', () => {
    expect(fixKey('C:\\code\\a', problem('p', '2.0.1'))).not.toBe(fixKey('C:\\code\\a', problem('p', '2.0.0')))
  })
})
