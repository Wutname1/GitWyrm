import { describe, expect, it } from 'vitest'
import type { ScannedExternalSession } from '@/lib/bindings'
import {
  buildImportRows,
  filterImportSessions,
  importDayBucket,
  isImported,
  nextSelection,
  projectChoices,
  summarizeSelection,
  toggleSelectAllVisible,
} from '@/lib/agentImportList'

/**
 * Bulk selection, filtering, day grouping, and the dedupe-facing bits of the
 * "Import chats" surface.
 *
 * These cover the behavior that made the old surface unusable at real scale:
 * importing hundreds of chats one click at a time, and a select-all that could
 * silently reach past a filter into chats nobody had seen.
 */

/** Fixtures are built from local-time day offsets, not fixed UTC strings.
 * `importDayBucket` buckets by the viewer's own day boundary (a chat finished
 * late last night should read as "Yesterday" to the person who wrote it), so a
 * hard-coded UTC hour lands in a different bucket depending on where the test
 * runs. */
const NOW = new Date(2026, 8, 12, 12, 0, 0).getTime()

function daysAgo(days: number, hour = 9): string {
  const d = new Date(NOW)
  d.setDate(d.getDate() - days)
  d.setHours(hour, 0, 0, 0)
  return d.toISOString()
}

function session(
  id: string,
  {
    title = id,
    updatedAt = '2026-09-12T09:00:00Z',
    project = 'C:/code/GitWyrm',
    projectKind = 'resolved' as 'resolved' | 'unresolved' | 'noProjectRecorded',
    importedSessionId = null as string | null,
    messageCount = 4,
  } = {}
): ScannedExternalSession {
  const resolution =
    projectKind === 'resolved'
      ? {
          kind: 'resolved' as const,
          repoId: `id:${project}`,
          repoPath: project,
          // Derived from the path, so two fixtures with different projects do
          // not both claim to be named GitWyrm -- which would make a search by
          // project name look like it matched too much.
          repoName: project.split('/').pop() ?? project,
        }
      : projectKind === 'unresolved'
        ? { kind: 'unresolved' as const, recordedPath: project }
        : { kind: 'noProjectRecorded' as const }

  return {
    adapterId: 'claude-code',
    summary: {
      externalSessionId: id,
      title,
      updatedAt,
      projectPath: projectKind === 'noProjectRecorded' ? null : project,
      messageCount,
      model: null,
    },
    project: resolution,
    alreadyImported: importedSessionId != null,
    importedSessionId,
  } as ScannedExternalSession
}

describe('picking many chats at once', () => {
  const order = ['a', 'b', 'c', 'd', 'e']

  it('a plain click picks just that chat', () => {
    const result = nextSelection('c', { shift: false, ctrl: false }, {
      order,
      selected: new Set(['a', 'b']),
      anchor: 'a',
    })
    expect(result.selected).toEqual(['c'])
    expect(result.anchor).toBe('c')
  })

  it('shift-click takes everything between the anchor and the click', () => {
    const result = nextSelection('d', { shift: true, ctrl: false }, {
      order,
      selected: new Set(['b']),
      anchor: 'b',
    })
    expect(result.selected).toEqual(['b', 'c', 'd'])
    // The anchor stays put so the same range can be widened or narrowed by
    // shift-clicking again, rather than walking away from where it started.
    expect(result.anchor).toBe('b')
  })

  it('shift-click ranges backwards too', () => {
    const result = nextSelection('a', { shift: true, ctrl: false }, {
      order,
      selected: new Set(['c']),
      anchor: 'c',
    })
    expect(result.selected).toEqual(['a', 'b', 'c'])
  })

  it('a second shift-range joins the first instead of replacing it', () => {
    const first = nextSelection('b', { shift: true, ctrl: false }, {
      order,
      selected: new Set(['a']),
      anchor: 'a',
    })
    const second = nextSelection('e', { shift: true, ctrl: false }, {
      order,
      selected: new Set(first.selected),
      anchor: 'd',
    })
    expect(second.selected).toEqual(['a', 'b', 'd', 'e'])
  })

  it('ctrl-click adds one chat and ctrl-clicking it again removes it', () => {
    const added = nextSelection('d', { shift: false, ctrl: true }, {
      order,
      selected: new Set(['a']),
      anchor: 'a',
    })
    expect(added.selected).toEqual(['a', 'd'])

    const removed = nextSelection('d', { shift: false, ctrl: true }, {
      order,
      selected: new Set(added.selected),
      anchor: added.anchor,
    })
    expect(removed.selected).toEqual(['a'])
  })

  it('removing the anchor hands the role to a chat still picked', () => {
    const result = nextSelection('c', { shift: false, ctrl: true }, {
      order,
      selected: new Set(['a', 'c']),
      anchor: 'c',
    })
    expect(result.selected).toEqual(['a'])
    // Without this the next shift-click has nothing to range from and
    // silently degrades to a plain click.
    expect(result.anchor).toBe('a')
  })

  it('keeps the selection in the order it is shown, not click order', () => {
    const result = nextSelection('a', { shift: false, ctrl: true }, {
      order,
      selected: new Set(['e', 'c']),
      anchor: 'e',
    })
    expect(result.selected).toEqual(['a', 'c', 'e'])
  })

  it('drops chats a filter has taken off screen', () => {
    const result = nextSelection('b', { shift: false, ctrl: true }, {
      order: ['a', 'b'],
      selected: new Set(['a', 'gone']),
      anchor: 'a',
    })
    expect(result.selected).toEqual(['a', 'b'])
  })

  it('a shift-click with no anchor falls back to picking one chat', () => {
    const result = nextSelection('c', { shift: true, ctrl: false }, {
      order,
      selected: new Set(),
      anchor: null,
    })
    expect(result.selected).toEqual(['c'])
  })
})

describe('pick all / clear', () => {
  it('picks every chat currently on screen', () => {
    const result = toggleSelectAllVisible(['a', 'b', 'c'], new Set())
    expect(result.selected.sort()).toEqual(['a', 'b', 'c'])
  })

  it('clears them when they are all already picked', () => {
    const result = toggleSelectAllVisible(['a', 'b'], new Set(['a', 'b']))
    expect(result.selected).toEqual([])
  })

  /**
   * The one that matters. Pick-all is scoped to what a filter is showing, so
   * it can never reach past the search into chats nobody has seen -- which is
   * how a two-hundred-chat import happens by accident.
   */
  it('never reaches past the filter into chats that are not on screen', () => {
    const onScreen = ['a', 'b']
    const result = toggleSelectAllVisible(onScreen, new Set())
    expect(result.selected).not.toContain('hidden-1')
    expect(result.selected).toHaveLength(2)
  })

  it('keeps chats picked under an earlier filter when clearing these', () => {
    const result = toggleSelectAllVisible(['a', 'b'], new Set(['a', 'b', 'picked-earlier']))
    expect(result.selected).toEqual(['picked-earlier'])
  })
})

describe('what one press will do', () => {
  const sessions = [
    session('new-1'),
    session('new-2'),
    session('already-in', { importedSessionId: 'gw-1' }),
  ]

  /**
   * Chats already brought in are refreshed, not duplicated, and the button has
   * to say so. "Bring in 3 chats" when one of them is a refresh describes work
   * it is not doing.
   */
  it('separates brand-new chats from ones already brought in', () => {
    const tally = summarizeSelection(['new-1', 'new-2', 'already-in'], sessions)
    expect(tally).toEqual({ total: 3, newChats: 2, refreshes: 1 })
  })

  it('ignores picked chats that are no longer in the scan', () => {
    const tally = summarizeSelection(['new-1', 'vanished'], sessions)
    expect(tally.total).toBe(1)
  })

  it('reads a chat with a linked GitWyrm chat as already brought in', () => {
    expect(isImported(session('x', { importedSessionId: 'gw-9' }))).toBe(true)
    expect(isImported(session('y'))).toBe(false)
  })
})

describe('finding one chat in hundreds', () => {
  const sessions = [
    session('a', { title: 'Fix the migration', updatedAt: '2026-09-12T08:00:00Z' }),
    session('b', { title: 'Rename a branch', updatedAt: '2026-09-11T08:00:00Z' }),
    session('c', {
      title: 'Unrelated',
      updatedAt: '2026-09-08T08:00:00Z',
      project: 'C:/code/Other',
    }),
    session('d', {
      title: 'Missing folder work',
      updatedAt: '2026-08-01T08:00:00Z',
      project: 'C:/gone/Away',
      projectKind: 'unresolved',
    }),
  ]

  it('searches titles', () => {
    const found = filterImportSessions(sessions, { search: 'migration' })
    expect(found.map((s) => s.summary.externalSessionId)).toEqual(['a'])
  })

  it('searches the project name shown beside the title', () => {
    const found = filterImportSessions(sessions, { search: 'gitwyrm' })
    expect(found.map((s) => s.summary.externalSessionId)).toEqual(['a', 'b'])
  })

  it('narrows to one project', () => {
    const found = filterImportSessions(sessions, {
      project: { kind: 'path', path: 'C:/code/Other' },
    })
    expect(found.map((s) => s.summary.externalSessionId)).toEqual(['c'])
  })

  it('narrows to chats whose folder could not be found', () => {
    const found = filterImportSessions(sessions, { project: { kind: 'unresolved' } })
    expect(found.map((s) => s.summary.externalSessionId)).toEqual(['d'])
  })

  it('hides chats already brought in when asked', () => {
    const withImported = [...sessions, session('e', { importedSessionId: 'gw-2' })]
    const found = filterImportSessions(withImported, { imported: 'notImported' })
    expect(found.map((s) => s.summary.externalSessionId)).not.toContain('e')
  })

  it('always returns newest first, whatever order the scan arrived in', () => {
    const shuffled = [sessions[3], sessions[1], sessions[0], sessions[2]]
    const found = filterImportSessions(shuffled)
    expect(found.map((s) => s.summary.externalSessionId)).toEqual(['a', 'b', 'c', 'd'])
  })

  it('sinks chats with no recorded date instead of scattering them', () => {
    const undated = session('undated', { updatedAt: 'not a date' })
    const found = filterImportSessions([undated, ...sessions])
    expect(found[found.length - 1]?.summary.externalSessionId).toBe('undated')
  })
})

describe('grouping by day', () => {
  it('buckets by the chat own last-changed time', () => {
    expect(importDayBucket(daysAgo(0), NOW)).toBe('Today')
    expect(importDayBucket(daysAgo(1), NOW)).toBe('Yesterday')
    expect(importDayBucket(daysAgo(4), NOW)).toBe('This week')
    expect(importDayBucket(daysAgo(60), NOW)).toBe('Earlier')
  })

  /** A client that records no date must not have its chats dated today, or a
   * years-old conversation tops the list on every open. */
  it('puts an undated chat in Earlier, never Today', () => {
    expect(importDayBucket('', NOW)).toBe('Earlier')
    expect(importDayBucket('no date recorded', NOW)).toBe('Earlier')
  })

  it('puts a header before each day and keeps chats under it', () => {
    const rows = buildImportRows(
      [session('a', { updatedAt: daysAgo(0) }), session('b', { updatedAt: daysAgo(1) })],
      { collapsedGroupIds: new Set(), now: NOW }
    )
    expect(rows.map((r) => (r.kind === 'header' ? `# ${r.label}` : r.id))).toEqual([
      '# Today',
      'a',
      '# Yesterday',
      'b',
    ])
  })

  it('keeps a collapsed day header so it can be opened again', () => {
    const rows = buildImportRows([session('a', { updatedAt: daysAgo(0) })], {
      collapsedGroupIds: new Set(['Today']),
      now: NOW,
    })
    expect(rows).toHaveLength(1)
    expect(rows[0]).toMatchObject({ kind: 'header', label: 'Today', count: 1, collapsed: true })
  })

  it('emits no header for a day with no chats', () => {
    const rows = buildImportRows([session('a', { updatedAt: daysAgo(0) })], {
      collapsedGroupIds: new Set(),
      now: NOW,
    })
    expect(rows.filter((r) => r.kind === 'header')).toHaveLength(1)
  })
})

describe('the project filter list', () => {
  it('counts chats per project, busiest first', () => {
    const choices = projectChoices([
      session('a', { project: 'C:/code/GitWyrm' }),
      session('b', { project: 'C:/code/GitWyrm' }),
      session('c', { project: 'C:/code/Other' }),
    ])
    expect(choices[0]).toMatchObject({ count: 2 })
    expect(choices[1]).toMatchObject({ count: 1 })
  })

  it('flags a project whose folder could not be found', () => {
    const choices = projectChoices([
      session('d', { project: 'C:/gone', projectKind: 'unresolved' }),
    ])
    expect(choices[0]).toMatchObject({ unresolved: true })
  })

  it('groups chats with no recorded project together', () => {
    const choices = projectChoices([
      session('a', { projectKind: 'noProjectRecorded' }),
      session('b', { projectKind: 'noProjectRecorded' }),
    ])
    expect(choices).toHaveLength(1)
    expect(choices[0]).toMatchObject({ name: 'No project recorded', count: 2 })
  })
})

/**
 * Two things sync must not do, checked against the source because the wiring
 * that decides them lives in a component and this project has no DOM test
 * environment (`vitest.config.ts`).
 *
 * Both were real defects in this rebuild, caught on review rather than by a
 * test, which is exactly why they are pinned now: neither produces an error,
 * and both are invisible until someone is midway through picking chats.
 */
describe('keeping in sync stays out of the way', () => {
  function importPickerSource(): string {
    // @ts-expect-error -- no @types/node in this project; available at runtime
    const { readFileSync } = require('node:fs')
    // @ts-expect-error -- no @types/node in this project; available at runtime
    const { fileURLToPath } = require('node:url')
    const path = fileURLToPath(
      new URL('../components/domain/agent-desk/ImportPicker.tsx', import.meta.url)
    )
    return readFileSync(path, 'utf8')
  }

  /**
   * Sync runs on a timer. If it cleared the selection the way a pressed import
   * does, it would reach in every few minutes and discard chats someone was
   * halfway through picking, with no action of theirs to explain it.
   */
  it('a timed import never clears a selection the person is building', () => {
    const source = importPickerSource()
    // The clear inside the batch handler is guarded by "this batch was pressed
    // for", rather than running at the end of every batch. The Clear button's
    // own call is deliberately not covered here -- that one IS the person
    // asking.
    expect(source).toMatch(/if\s*\(!syncedFrom\)\s*\{[\s\S]{0,120}setSelectedIds\(\[\]\)/)
    // And the guard is inside the batch result handler, not somewhere it
    // could never run.
    expect(source).toMatch(/summarizeBatchImport[\s\S]{0,1400}if\s*\(!syncedFrom\)/)
  })

  /**
   * The poll and the import effect read `onImport`/`refetch` through refs. Both
   * change identity on nearly every render, so listing them as dependencies
   * rebuilds the interval before it can ever fire -- the toggle would look on
   * and never actually check.
   */
  it('the timer is not rebuilt on every render', () => {
    const source = importPickerSource()
    expect(source).toContain('refetchRef.current()')
    expect(source).toMatch(/SYNC_POLL_MS\)\s*\n\s*return \(\) => window\.clearInterval\(id\)\s*\n\s*\},\s*\[syncOn, enabled\]\)/)
  })
})
