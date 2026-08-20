import { describe, expect, it } from 'vitest'
import {
  clientLabel,
  eligibleDestinationsFor,
  formatSummaryLine,
  hasAnyDifference,
  isEligibleDestination,
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
