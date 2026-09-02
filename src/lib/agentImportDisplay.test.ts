import { describe, expect, it } from 'vitest'
import {
  canBrowseAdapter,
  continueExternallyLabel,
  detectionLabel,
  linkedImportedSessionId,
  projectLabel,
  unlinkConfirmCopy,
} from './agentImportDisplay'
import type {
  AdapterListEntry,
  AgentSession,
  ContinuationOutcome,
  ImportSessionOutcome,
  ScannedExternalSession,
} from '@/lib/bindings'

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
    importedSessionId: null,
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

describe('linkedImportedSessionId', () => {
  // Only the header field the helper reads is filled in; the cast keeps the
  // fixture honest about being a partial rather than mocking a full session.
  const importedOutcome = (sessionId: string): ImportSessionOutcome => ({
    kind: 'created',
    session: { header: { sessionId } } as unknown as AgentSession,
  })

  it('is null for a never-imported row with no import result', () => {
    expect(linkedImportedSessionId(scanned(), undefined)).toBeNull()
  })

  it('uses the ledger id the scan already knows', () => {
    expect(
      linkedImportedSessionId(scanned({ alreadyImported: true, importedSessionId: 'sess-1' }), undefined)
    ).toBe('sess-1')
  })

  it("prefers this row's fresh import result over a stale scan", () => {
    expect(linkedImportedSessionId(scanned(), importedOutcome('sess-new'))).toBe('sess-new')
  })

  it('ignores a failed import result', () => {
    expect(linkedImportedSessionId(scanned(), { kind: 'sessionNotFound' })).toBeNull()
  })
})

describe('unlinkConfirmCopy', () => {
  it('names the client and promises the messages stay', () => {
    const copy = unlinkConfirmCopy('Codex')
    expect(copy.title).toBe('Unlink from Codex?')
    expect(copy.description).toContain('stays in GitWyrm')
    expect(copy.description).toContain('Codex')
    expect(copy.description).not.toMatch(/type .* to confirm/i)
  })
})
