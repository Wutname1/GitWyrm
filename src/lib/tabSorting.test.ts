import { describe, expect, it } from 'vitest'
import { arrangeTabs, bucketByRecency, recencyBucket } from './tabSorting'
import { pathKey } from './paths'
import type { TabGroup, TabOrderItem } from '@/stores/workspaceStore'

/** 2026-08-17, 14:30 local -- a mid-afternoon "now" so same-day maths is not
 * sitting on a midnight boundary by accident. */
const NOW = new Date(2026, 7, 17, 14, 30).getTime()
const at = (
  year: number,
  month: number,
  day: number,
  hour = 12,
  minute = 0,
): number => new Date(year, month, day, hour, minute).getTime()

const repo = (path: string): TabOrderItem => ({ type: 'repo', path })

/** Stamps are keyed the way the store keys them, never hand-written. */
const stamps = (entries: Record<string, number>): Record<string, number> =>
  Object.fromEntries(
    Object.entries(entries).map(([path, time]) => [pathKey(path), time]),
  )

function arrangeRecent(
  order: TabOrderItem[],
  lastUsedAt: Record<string, number>,
  extra: { groups?: TabGroup[]; direction?: 'forward' | 'reverse' } = {},
) {
  return arrangeTabs({
    order,
    groups: extra.groups ?? [],
    sort: 'recent',
    direction: extra.direction,
    pinned: [],
    aliases: {},
    names: {},
    changeCounts: {},
    lastUsedAt,
  })
}

const pathsOf = (items: TabOrderItem[]) =>
  items.map((item) => (item as { path: string }).path)

describe('recencyBucket', () => {
  it('files this morning and just now under Today', () => {
    expect(recencyBucket(at(2026, 7, 17, 8, 0), NOW).label).toBe('Today')
    expect(recencyBucket(NOW, NOW).label).toBe('Today')
  })

  it('counts calendar days, not rolling 24-hour windows', () => {
    // 11pm last night is ~15 hours ago -- under a day, but still Yesterday.
    expect(recencyBucket(at(2026, 7, 16, 23, 0), NOW).label).toBe('Yesterday')
    // 1am today is ~13 hours ago and must not borrow yesterday's label.
    expect(recencyBucket(at(2026, 7, 17, 1, 0), NOW).label).toBe('Today')
  })

  it('gives days two through six their own dated heading', () => {
    const twoDaysAgo = recencyBucket(at(2026, 7, 15), NOW)
    expect(twoDaysAgo.label).toContain('Aug 15')
    expect(twoDaysAgo.id).not.toBe('older')

    const sixDaysAgo = recencyBucket(at(2026, 7, 11), NOW)
    expect(sixDaysAgo.label).toContain('Aug 11')
    expect(sixDaysAgo.id).not.toBe('older')
  })

  it('collapses day seven and everything older into one heading', () => {
    expect(recencyBucket(at(2026, 7, 10), NOW).label).toBe('Over a Week or Older')
    expect(recencyBucket(at(2025, 0, 1), NOW).label).toBe('Over a Week or Older')
  })

  it('treats a missing or junk stamp as oldest', () => {
    expect(recencyBucket(undefined, NOW).id).toBe('older')
    expect(recencyBucket(0, NOW).id).toBe('older')
    expect(recencyBucket(Number.NaN, NOW).id).toBe('older')
  })

  it('keeps a future stamp at the top rather than exiling it', () => {
    expect(recencyBucket(at(2026, 7, 20), NOW).label).toBe('Today')
  })
})

describe('arrangeTabs, recently used', () => {
  it('puts the most recently used tab first', () => {
    const { rest } = arrangeRecent(
      [repo('C:/code/alpha'), repo('C:/code/beta'), repo('C:/code/gamma')],
      stamps({
        'C:/code/alpha': at(2026, 7, 10),
        'C:/code/beta': NOW,
        'C:/code/gamma': at(2026, 7, 16),
      }),
    )
    expect(pathsOf(rest)).toEqual([
      'C:/code/beta',
      'C:/code/gamma',
      'C:/code/alpha',
    ])
  })

  it('sinks never-used tabs to the bottom, ordered by name', () => {
    const { rest } = arrangeRecent(
      [repo('C:/code/zulu'), repo('C:/code/alpha'), repo('C:/code/used')],
      stamps({ 'C:/code/used': NOW }),
    )
    expect(pathsOf(rest)).toEqual([
      'C:/code/used',
      'C:/code/alpha',
      'C:/code/zulu',
    ])
  })

  it('reverses to stalest first', () => {
    const { rest } = arrangeRecent(
      [repo('C:/code/alpha'), repo('C:/code/beta')],
      stamps({ 'C:/code/alpha': at(2026, 7, 10), 'C:/code/beta': NOW }),
      { direction: 'reverse' },
    )
    expect(pathsOf(rest)[0]).toBe('C:/code/alpha')
  })

  it('floats a group by its most recently used member', () => {
    const groups: TabGroup[] = [
      {
        id: 'work',
        name: 'Work',
        color: '#2dd4bf',
        collapsed: false,
        repoPaths: ['C:/code/stale', 'C:/code/fresh'],
      },
    ]
    const { rest } = arrangeRecent(
      [repo('C:/code/loose'), { type: 'group', id: 'work' }],
      stamps({
        'C:/code/stale': at(2025, 0, 1),
        'C:/code/fresh': NOW,
        'C:/code/loose': at(2026, 7, 16),
      }),
      { groups },
    )
    expect(rest[0]).toEqual({ type: 'group', id: 'work' })
  })

  it('leaves pinned tabs out of the recency order', () => {
    const { pinned, rest } = arrangeTabs({
      order: [repo('C:/code/pinned'), repo('C:/code/fresh')],
      groups: [],
      sort: 'recent',
      pinned: ['C:/code/pinned'],
      aliases: {},
      names: {},
      changeCounts: {},
      lastUsedAt: stamps({
        'C:/code/pinned': at(2025, 0, 1),
        'C:/code/fresh': NOW,
      }),
    })
    expect(pathsOf(pinned)[0]).toBe('C:/code/pinned')
    expect(rest).toHaveLength(1)
  })
})

describe('bucketByRecency', () => {
  it('splits a sorted run into headed sections, skipping empty ones', () => {
    const lastUsedAt = stamps({
      'C:/code/now': NOW,
      'C:/code/today': at(2026, 7, 17, 9, 0),
      'C:/code/yesterday': at(2026, 7, 16),
      'C:/code/ancient': at(2025, 0, 1),
    })
    const { rest } = arrangeRecent(
      [
        repo('C:/code/now'),
        repo('C:/code/today'),
        repo('C:/code/yesterday'),
        repo('C:/code/ancient'),
      ],
      lastUsedAt,
    )
    const sections = bucketByRecency(rest, { groups: [], lastUsedAt, now: NOW })

    expect(sections.map((section) => section.bucket.label)).toEqual([
      'Today',
      'Yesterday',
      'Over a Week or Older',
    ])
    // Both of today's tabs share the one heading.
    expect(sections[0]!.items).toHaveLength(2)
  })

  it('buckets a group by its most recently used member', () => {
    const groups: TabGroup[] = [
      {
        id: 'work',
        name: 'Work',
        color: '#2dd4bf',
        collapsed: false,
        repoPaths: ['C:/code/stale', 'C:/code/fresh'],
      },
    ]
    const lastUsedAt = stamps({
      'C:/code/stale': at(2025, 0, 1),
      'C:/code/fresh': NOW,
    })
    const sections = bucketByRecency([{ type: 'group', id: 'work' }], {
      groups,
      lastUsedAt,
      now: NOW,
    })
    expect(sections[0]!.bucket.label).toBe('Today')
  })

  it('keeps headings contiguous when the sort is reversed', () => {
    const lastUsedAt = stamps({
      'C:/code/now': NOW,
      'C:/code/today': at(2026, 7, 17, 9, 0),
      'C:/code/ancient': at(2025, 0, 1),
    })
    const { rest } = arrangeRecent(
      [repo('C:/code/now'), repo('C:/code/today'), repo('C:/code/ancient')],
      lastUsedAt,
      { direction: 'reverse' },
    )
    const sections = bucketByRecency(rest, { groups: [], lastUsedAt, now: NOW })

    // Oldest first, and Today is still one section rather than two runs.
    expect(sections.map((section) => section.bucket.label)).toEqual([
      'Over a Week or Older',
      'Today',
    ])
    expect(sections[1]!.items).toHaveLength(2)
  })

  it('files a tab with no stamp under the oldest heading', () => {
    const sections = bucketByRecency([repo('C:/code/never')], {
      groups: [],
      lastUsedAt: {},
      now: NOW,
    })
    expect(sections[0]!.bucket.label).toBe('Over a Week or Older')
  })
})
