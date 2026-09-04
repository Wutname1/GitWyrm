import { describe, expect, it } from 'vitest'
import {
  itemDisplayName,
  summarizeInventoryCounts,
  describeConfigOperation,
  explainConfigUndoOutcome,
  CLIENT_COLUMN_ORDER,
  CLIENT_LABEL,
  clientLabel,
  eligibleDestinationsFor,
  formatSummaryLine,
  hasAnyDifference,
  isEligibleDestination,
  partitionBatchCandidates,
  syncBadgeClass,
  syncBadgeLabel,
} from './agentConfig'
import type { ClientSyncStatus, InventoryEntry, ClientSyncState } from '@/lib/bindings'
import type { InventorySummary } from './agentConfig'

const ALL_STATES: ClientSyncState[] = [
  'same',
  'different',
  'missing',
  'outdated',
  'isSource',
  'unsupported',
  'keptSeparate',
  'needsSetup',
  'disabled',
  'clientNotDetected',
]

describe('syncBadgeClass / syncBadgeLabel', () => {
  it('maps every known state to a badge class and a non-empty label', () => {
    for (const state of ALL_STATES) {
      expect(['good', 'warn', 'missing']).toContain(syncBadgeClass(state))
      expect(syncBadgeLabel(state).length).toBeGreaterThan(0)
    }
  })

  it('matches the mockup badge vocabulary exactly for the observed states', () => {
    expect(syncBadgeLabel('same')).toBe('Synced')
    expect(syncBadgeLabel('different')).toBe('Different')
    expect(syncBadgeLabel('missing')).toBe('Missing')
    expect(syncBadgeLabel('outdated')).toBe('Older')
    expect(syncBadgeLabel('isSource')).toBe('Source')
    expect(syncBadgeLabel('unsupported')).toBe('Unsupported')
    expect(syncBadgeLabel('keptSeparate')).toBe('Own copy')
    expect(syncBadgeLabel('needsSetup')).toBe('Needs setup')
    expect(syncBadgeLabel('disabled')).toBe('Off')
  })

  it('classifies good/warn/missing consistently with the mockup CSS classes', () => {
    expect(syncBadgeClass('same')).toBe('good')
    expect(syncBadgeClass('isSource')).toBe('good')
    expect(syncBadgeClass('keptSeparate')).toBe('good')
    expect(syncBadgeClass('different')).toBe('warn')
    expect(syncBadgeClass('outdated')).toBe('warn')
    expect(syncBadgeClass('needsSetup')).toBe('warn')
    expect(syncBadgeClass('missing')).toBe('missing')
    expect(syncBadgeClass('unsupported')).toBe('missing')
    expect(syncBadgeClass('disabled')).toBe('missing')
  })
})

describe('clientLabel', () => {
  it('labels every client id', () => {
    expect(clientLabel('codex')).toBe('Codex')
    expect(clientLabel('claude-code')).toBe('Claude')
    expect(clientLabel('open-code')).toBe('OpenCode')
    expect(clientLabel('vs-code-copilot')).toBe('Copilot')
  })
})

describe('explainConfigUndoOutcome', () => {
  // Undo used to report the same thing whether or not it put anything back.
  it('only reports restored when something was actually put back', () => {
    expect(explainConfigUndoOutcome({ kind: 'restored', receipt: {} as never }).restored).toBe(true)
    expect(explainConfigUndoOutcome({ kind: 'alreadyUndone' }).restored).toBe(false)
    expect(explainConfigUndoOutcome({ kind: 'operationNotFound' }).restored).toBe(false)
    expect(explainConfigUndoOutcome({ kind: 'restoreFailed', detail: 'disk full' }).restored).toBe(false)
  })

  it('says plainly that a newer edit was left alone, since the file is NOT back to how it was', () => {
    // The case that matters most: silence here means the person believes
    // another app's config was restored when it was deliberately not touched.
    const out = explainConfigUndoOutcome({
      kind: 'concurrentChangeRefused',
      expectedHash: 'aaa',
      actualHash: 'bbb',
    })
    expect(out.restored).toBe(false)
    expect(out.message).toMatch(/changed after/i)
  })

  it('carries the reason through when the restore itself failed', () => {
    expect(explainConfigUndoOutcome({ kind: 'restoreFailed', detail: 'disk full' }).message).toMatch(/disk full/)
  })
})

describe('CLIENT_COLUMN_ORDER', () => {
  // The invariant this file did not previously assert. A hand-written column
  // list dropped `open-chamber`, which has a real writer: it was pre-ticked as
  // an eligible destination, had no checkbox to untick, and had no column
  // showing its state -- so a copy could land in it without ever being
  // offered. That is the same "wrote to an app nobody selected" failure the
  // batch dialog was built to prevent, reintroduced one constant away.
  it('shows every client that has a name, so none can be written to unseen', () => {
    const named = Object.keys(CLIENT_LABEL).sort()
    expect([...CLIENT_COLUMN_ORDER].sort()).toEqual(named)
  })

  it('lists each client exactly once', () => {
    expect(new Set(CLIENT_COLUMN_ORDER).size).toBe(CLIENT_COLUMN_ORDER.length)
  })

  it('includes open-chamber specifically, the one that was missing', () => {
    expect(CLIENT_COLUMN_ORDER).toContain('open-chamber')
  })
})

describe('formatSummaryLine', () => {
  it('pluralizes correctly', () => {
    const one: InventorySummary = { total: 1, matching: 1, differing: 0, existsInOne: 0 }
    const many: InventorySummary = { total: 18, matching: 11, differing: 3, existsInOne: 4 }
    expect(formatSummaryLine(one, 'skill')).toBe('1 skill found')
    expect(formatSummaryLine(many, 'skill')).toBe('18 skills found')
    expect(formatSummaryLine(many, 'mcpConnector')).toBe('18 connectors found')
  })
})

function status(client: ClientSyncStatus['client'], state: ClientSyncState): ClientSyncStatus {
  return { client, state }
}

describe('isEligibleDestination', () => {
  it('excludes the source, an already-synced client, a deliberately-kept-separate one, and an undetected client', () => {
    expect(isEligibleDestination(status('codex', 'isSource'))).toBe(false)
    expect(isEligibleDestination(status('codex', 'same'))).toBe(false)
    expect(isEligibleDestination(status('codex', 'keptSeparate'))).toBe(false)
    expect(isEligibleDestination(status('codex', 'clientNotDetected'))).toBe(false)
  })

  it('includes different/missing/outdated/needsSetup as real destinations', () => {
    expect(isEligibleDestination(status('codex', 'different'))).toBe(true)
    expect(isEligibleDestination(status('codex', 'missing'))).toBe(true)
    expect(isEligibleDestination(status('codex', 'outdated'))).toBe(true)
    expect(isEligibleDestination(status('codex', 'needsSetup'))).toBe(true)
  })
})

function entry(perClient: ClientSyncStatus[]): InventoryEntry {
  return {
    itemId: 'McpConnector:github',
    kind: 'mcpConnector',
    displayName: 'github',
    scope: 'personal',
    source: { kind: 'client', client: 'codex' },
    perClient,
    hasSecrets: false,
  }
}

describe('eligibleDestinationsFor', () => {
  it('lists only clients that are real copy targets', () => {
    const row = entry([
      status('codex', 'isSource'),
      status('claude-code', 'different'),
      status('open-code', 'same'),
      status('vs-code-copilot', 'missing'),
    ])
    expect(eligibleDestinationsFor(row).sort()).toEqual(['claude-code', 'vs-code-copilot'].sort())
  })
})

describe('hasAnyDifference', () => {
  it('is false when every client matches or is the source', () => {
    const row = entry([status('codex', 'isSource'), status('claude-code', 'same')])
    expect(hasAnyDifference(row)).toBe(false)
  })

  it('is true when any client differs, is outdated, or is missing', () => {
    expect(hasAnyDifference(entry([status('codex', 'isSource'), status('claude-code', 'different')]))).toBe(true)
    expect(hasAnyDifference(entry([status('codex', 'isSource'), status('claude-code', 'missing')]))).toBe(true)
    expect(hasAnyDifference(entry([status('codex', 'isSource'), status('claude-code', 'outdated')]))).toBe(true)
  })
})

describe('partitionBatchCandidates', () => {
  // R7.5: "Match selected apps" must build one plan per differing item that
  // has somewhere eligible to go, and skip the rest honestly (never silently
  // drop them) -- this is the pure partition `BatchReviewDialog` drives its
  // per-item preview loop from.
  it('excludes entries that match everywhere or are the source everywhere', () => {
    const matching = entry([status('codex', 'isSource'), status('claude-code', 'same')])
    const { candidates, skipped } = partitionBatchCandidates([matching])
    expect(candidates).toEqual([])
    expect(skipped).toEqual([])
  })

  it('builds a candidate with its eligible destinations for a differing entry', () => {
    const differing = entry([
      status('codex', 'isSource'),
      status('claude-code', 'different'),
      status('open-code', 'same'),
      status('vs-code-copilot', 'missing'),
    ])
    const { candidates, skipped } = partitionBatchCandidates([differing])
    expect(skipped).toEqual([])
    expect(candidates).toHaveLength(1)
    expect(candidates[0].entry).toBe(differing)
    expect(candidates[0].destinations.sort()).toEqual(['claude-code', 'vs-code-copilot'].sort())
  })

  it('only lists the eligible destinations for a row, excluding ineligible clients even on a differing row', () => {
    const row = entry([status('codex', 'isSource'), status('claude-code', 'missing'), status('open-code', 'unsupported')])
    const { candidates, skipped } = partitionBatchCandidates([row])
    expect(hasAnyDifference(row)).toBe(true) // driven by the 'missing' status alone
    expect(candidates).toHaveLength(1)
    // 'open-code' is unsupported (never a real destination) and must not
    // appear even though the row as a whole differs.
    expect(candidates[0].destinations).toEqual(['claude-code'])
    expect(skipped).toEqual([])
  })

  it('does not even consider an entry with no difference at all -- it is neither a candidate nor skipped', () => {
    const untouched = entry([status('codex', 'isSource'), status('claude-code', 'unsupported')])
    expect(hasAnyDifference(untouched)).toBe(false)
    const { candidates, skipped } = partitionBatchCandidates([untouched])
    expect(candidates).toEqual([])
    expect(skipped).toEqual([])
  })

  it('processes multiple differing entries independently, building a candidate for each', () => {
    // Every state hasAnyDifference triggers on (different/outdated/missing)
    // is itself also eligible per isEligibleDestination, so in practice a
    // row that differs always has somewhere to go -- `skipped` exists for
    // the async-failure path in BatchReviewDialog (a preview call that
    // errors or comes back empty), not for this synchronous partition. This
    // test documents that invariant rather than asserting a skip that this
    // pure function cannot actually produce from static states alone.
    const first = entry([status('codex', 'isSource'), status('claude-code', 'different')])
    const second = entry([status('codex', 'isSource'), status('open-code', 'outdated')])
    const { candidates, skipped } = partitionBatchCandidates([first, second])
    expect(candidates.map((c) => c.entry)).toEqual([first, second])
    expect(skipped).toEqual([])
  })
})

describe('describeConfigOperation', () => {
  it('says whether the file already existed', () => {
    expect(describeConfigOperation({ client: 'Copilot', beforeHash: 'abc', undone: false })).toBe(
      'Changed settings for Copilot'
    )
    expect(describeConfigOperation({ client: 'Copilot', beforeHash: null, undone: false })).toBe(
      'Created settings for Copilot'
    )
  })
  it('says when a change has already been put back', () => {
    expect(describeConfigOperation({ client: 'Codex', beforeHash: 'abc', undone: true })).toBe(
      'Changed settings for Codex (already put back)'
    )
  })
})

describe('summarizeInventoryCounts', () => {
  const row = (states: string[]) =>
    ({ kind: 'skill', perClient: states.map((state) => ({ state })) }) as unknown as Parameters<
      typeof summarizeInventoryCounts
    >[0][number]

  it('never counts one item under two headings', () => {
    // An item living in exactly one app used to satisfy BOTH "match" and
    // "exists in one app", so the numbers could add up past the total.
    const rows = [row(['isSource', 'clientNotDetected'])]
    const s = summarizeInventoryCounts(rows)
    expect(s.matching + s.differing + s.existsInOne).toBeLessThanOrEqual(s.total)
    expect(s.existsInOne).toBe(1)
    expect(s.matching).toBe(0)
  })

  it('reports a differing item as differing whatever else is true of it', () => {
    const s = summarizeInventoryCounts([row(['different', 'missing'])])
    expect(s.differing).toBe(1)
    expect(s.existsInOne).toBe(0)
    expect(s.matching).toBe(0)
  })

  it('counts a genuine match as a match', () => {
    const s = summarizeInventoryCounts([row(['same', 'same'])])
    expect(s.matching).toBe(1)
    expect(s.differing + s.existsInOne).toBe(0)
  })
})

describe('itemDisplayName', () => {
  const entries = [{ itemId: 'McpConnector:github', displayName: 'GitHub' }] as unknown as Parameters<
    typeof itemDisplayName
  >[0]

  it('gives the name a person recognises, not the internal id', () => {
    expect(itemDisplayName(entries, 'McpConnector:github')).toBe('GitHub')
  })
  it('falls back to the id rather than to nothing', () => {
    // An unrecognisable heading still beats a blank one.
    expect(itemDisplayName(entries, 'Skill:unknown')).toBe('Skill:unknown')
  })
})

describe('describeConfigOperation names apps the way the user sees them', () => {
  it('turns the stored key into the display name', () => {
    // A receipt stores `vs-code-copilot`, deliberately, so a future build can
    // still read it back to undo. The permanent record of what GitWyrm
    // changed was printing that key at the person.
    expect(describeConfigOperation({ client: 'vs-code-copilot', beforeHash: 'a', undone: false })).toBe(
      'Changed settings for Copilot'
    )
    expect(describeConfigOperation({ client: 'open-chamber', beforeHash: null, undone: false })).toBe(
      'Created settings for OpenChamber'
    )
  })
  it('falls back to the key for a client this build does not know', () => {
    // Which is exactly what a receipt from a future build would carry.
    expect(describeConfigOperation({ client: 'some-new-tool', beforeHash: 'a', undone: false })).toMatch(
      /some-new-tool/
    )
  })
})
