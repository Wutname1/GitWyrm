import { describe, expect, it } from 'vitest'
import {
  canBrowseAdapter,
  continueExternallyLabel,
  detectionLabel,
  projectLabel,
} from './agentImportDisplay'
import type { AdapterListEntry, ContinuationOutcome, ScannedExternalSession } from '@/lib/bindings'

function entry(overrides: Partial<AdapterListEntry> = {}): AdapterListEntry {
  return {
    adapterId: 'codex',
    displayName: 'Codex',
    enabled: true,
    supportedVersionRange: '>=0.100.0, <1.0.0',
    detection: { kind: 'detected', version: '0.142.5', supported: true },
    ...overrides,
  }
}

function scanned(overrides: Partial<ScannedExternalSession> = {}): ScannedExternalSession {
  return {
    adapterId: 'codex',
    summary: {
      externalSessionId: 'ext-1',
      title: 'Fixture session',
      updatedAt: '2026-01-01T00:00:00Z',
      projectPath: 'C:/code/fixture-project',
      messageCount: 2,
      model: 'gpt-5-test',
    },
    project: { kind: 'resolved', repoId: 'repo-1', repoName: 'fixture-project', repoPath: 'C:/code/fixture-project' },
    alreadyImported: false,
    ...overrides,
  }
}

describe('detectionLabel', () => {
  it('says Found for a supported, enabled, detected adapter', () => {
    expect(detectionLabel(entry())).toBe('Found')
  })

  it('flags an older version distinctly', () => {
    expect(
      detectionLabel(entry({ detection: { kind: 'detected', version: '0.42.0', supported: false } }))
    ).toBe('Found (older version)')
  })

  it('says not supported yet for a detected but disabled adapter, not "not found"', () => {
    expect(
      detectionLabel(
        entry({ enabled: false, detection: { kind: 'detected', version: null, supported: false } })
      )
    ).toBe('Found, not supported yet')
  })

  it('says not found for a genuinely undetected client', () => {
    expect(detectionLabel(entry({ detection: { kind: 'notDetected' } }))).toBe(
      'Not found on this computer'
    )
  })

  it('says could not check for a failed detection, never fabricating a state', () => {
    expect(detectionLabel(entry({ detection: { kind: 'failed', detail: 'boom' } }))).toBe(
      'Could not check'
    )
  })
})

describe('canBrowseAdapter', () => {
  it('allows browsing a detected, enabled, supported adapter', () => {
    expect(canBrowseAdapter(entry())).toBe(true)
  })

  it('blocks browsing a detected but disabled adapter (e.g. OpenChamber)', () => {
    expect(
      canBrowseAdapter(
        entry({ enabled: false, detection: { kind: 'detected', version: null, supported: false } })
      )
    ).toBe(false)
  })

  it('blocks browsing an undetected adapter', () => {
    expect(canBrowseAdapter(entry({ detection: { kind: 'notDetected' } }))).toBe(false)
  })

  it('blocks browsing a failed detection', () => {
    expect(canBrowseAdapter(entry({ detection: { kind: 'failed', detail: 'x' } }))).toBe(false)
  })
})

describe('projectLabel', () => {
  it('shows the repo name for a resolved project', () => {
    const label = projectLabel(scanned())
    expect(label).toEqual({ text: 'fixture-project', resolved: true, offerLinking: false })
  })

  it('shows a distinct message when no project was recorded at all, and never offers linking', () => {
    const label = projectLabel(scanned({ project: { kind: 'noProjectRecorded' } }))
    expect(label.resolved).toBe(false)
    expect(label.offerLinking).toBe(false)
    expect(label.text).toBe('No project recorded')
  })

  it('keeps the recorded path visible for an unresolved project and offers linking (spec: keep unresolved paths visible)', () => {
    const label = projectLabel(
      scanned({ project: { kind: 'unresolved', recordedPath: 'C:/code/moved-project' } })
    )
    expect(label.resolved).toBe(false)
    expect(label.offerLinking).toBe(true)
    expect(label.text).toContain('C:/code/moved-project')
  })
})

describe('continueExternallyLabel', () => {
  it('returns null when there is no continuation data yet', () => {
    expect(continueExternallyLabel(undefined)).toBeNull()
  })

  it('says Open client for openOnly, never "Continue session" (spec: Continuation is honest)', () => {
    const outcome: ContinuationOutcome = { kind: 'openOnly' }
    expect(continueExternallyLabel(outcome)).toBe('Open client')
  })

  it('returns null for unsupported', () => {
    const outcome: ContinuationOutcome = { kind: 'unsupported' }
    expect(continueExternallyLabel(outcome)).toBeNull()
  })

  it('returns null for clientNotDetected', () => {
    const outcome: ContinuationOutcome = { kind: 'clientNotDetected' }
    expect(continueExternallyLabel(outcome)).toBeNull()
  })

  it('returns null for adapterDisabled', () => {
    const outcome: ContinuationOutcome = { kind: 'adapterDisabled' }
    expect(continueExternallyLabel(outcome)).toBeNull()
  })
})
