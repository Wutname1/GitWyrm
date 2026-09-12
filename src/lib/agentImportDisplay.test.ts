import { describe, expect, it } from 'vitest'
import {
  IMPORT_SCAN_STALE_MS,
  importedSessionId,
  isUnresolvedProject,
  explainImportScanRefusal,
  explainImportOutcome, adapterDisplayName, canBrowseAdapter,
  continueExternallyLabel,
  detectionLabel,
  linkedImportedSessionId,
  projectLabel,
  unlinkConfirmCopy,
  summarizeBatchImport,
  explainBatchRefusal,
  syncToggleCopy,
  explainSyncImport,
  SYNC_POLL_MS,
} from './agentImportDisplay'
import type {
  AdapterListEntry,
  BatchImportItem,
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

describe('IMPORT_SCAN_STALE_MS', () => {
  /**
   * The scan had no staleness at all, so it refetched on every window
   * focus -- rebuilding the row list, and with it unmounting any row whose
   * copy was still running. Both reads walk the same filesystem, so they
   * should agree about how often that is worth doing.
   */
  it('is long enough that looking at the window does not rescan', () => {
    expect(IMPORT_SCAN_STALE_MS).toBeGreaterThanOrEqual(60 * 1000)
  })

  /** And short enough that a chat added in the other tool turns up. */
  it('is short enough to notice a new chat without restarting', () => {
    expect(IMPORT_SCAN_STALE_MS).toBeLessThanOrEqual(10 * 60 * 1000)
  })
})

describe('importedSessionId', () => {
  const withSession = (kind: 'created' | 'refreshed', sessionId: string) =>
    ({
      kind,
      session: { header: { sessionId } },
      ...(kind === 'refreshed' ? { newMessageCount: 4 } : {}),
    }) as never

  /**
   * A refresh appends everything found since last time, so a chat already
   * open on screen has to be told. Without this the person was shown
   * "Added 4 new messages" above a transcript that still ended where it had
   * before.
   */
  it('names the session for an import that changed one', () => {
    expect(importedSessionId(withSession('created', 's1'))).toBe('s1')
    expect(importedSessionId(withSession('refreshed', 's2'))).toBe('s2')
  })

  /** Nothing changed, so nothing needs refreshing. */
  it('names nothing for an import that did not happen', () => {
    for (const kind of [
      'adapterDisabled',
      'clientNotDetected',
      'sessionNotFound',
      'ambiguousSession',
    ] as const) {
      expect(importedSessionId({ kind } as never)).toBeNull()
    }
    expect(importedSessionId({ kind: 'corruptSession', detail: 'x' } as never)).toBeNull()
    expect(importedSessionId({ kind: 'writeFailed', detail: 'x' } as never)).toBeNull()
  })
})

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

  it('never says "Continue session" for openOnly (spec: Continuation is honest)', () => {
    const outcome: ContinuationOutcome = { kind: 'openOnly' }
    const label = continueExternallyLabel(outcome)
    expect(label).not.toMatch(/continue session/i)
    // It also must not read as a button GitWyrm can press: no launch command
    // exists, so an imperative label promised an action nothing performs.
    expect(label).toBe('Can be continued in its own app')
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

describe('adapterDisplayName', () => {
  // The badge's tooltip showed the raw id -- "Imported from vscode-copilot"
  // -- which attributes to a slug rather than to a product. These ids come
  // from each adapter's `fn id()` in src-tauri/src/agentdesk/adapters/, and
  // the names from its `fn display_name()`; the two sides must not drift.
  it('names every adapter the way its own Rust side does', () => {
    expect(adapterDisplayName('claude-code')).toBe('Claude Code')
    expect(adapterDisplayName('codex')).toBe('Codex')
    expect(adapterDisplayName('opencode')).toBe('opencode')
    expect(adapterDisplayName('vscode-copilot')).toBe('VS Code Copilot Chat')
    expect(adapterDisplayName('openchamber')).toBe('OpenChamber')
  })

  it('says the unfamiliar thing it knows rather than inventing a name', () => {
    // An adapter from a later build should not be given a made-up label.
    expect(adapterDisplayName('some-future-client')).toBe('some-future-client')
  })
})

describe('explainImportOutcome', () => {
  /**
   * Two saved chats sharing one id is not a damaged file. Saying "could not
   * be read" would send someone hunting for a fault that is not there --
   * the other tool simply reused an id across two of its own folders, which
   * a restored backup or a synced profile can do.
   */
  it('does not call two chats with the same id a damaged file', () => {
    const { message, ok } = explainImportOutcome({ kind: 'ambiguousSession' }, 'Fix login', 'vs-code-copilot')
    expect(ok).toBe(false)
    expect(message).not.toMatch(/could not be read|corrupt|damaged/i)
    expect(message).toMatch(/cannot tell which one/i)
  })

  /** Rule #2: read by someone who does not know what any of this is called. */
  it('explains it without naming anything internal', () => {
    const { message } = explainImportOutcome({ kind: 'ambiguousSession' }, 'Fix login', 'vs-code-copilot')
    for (const word of ['session', 'adapter', 'id ', 'workspace hash', 'JSON']) {
      expect(message.toLowerCase()).not.toContain(word.toLowerCase())
    }
  })

  const session = {} as never

  it('distinguishes a refresh that brought something from one that brought nothing', () => {
    // `newMessageCount` was computed by the backend so the UI could say, and
    // nothing read it -- twelve new messages and none looked identical.
    const some = explainImportOutcome({ kind: 'refreshed', session, newMessageCount: 12 }, 'Fix login', 'codex')
    expect(some.message).toMatch(/Added 12 new messages/)
    const none = explainImportOutcome({ kind: 'refreshed', session, newMessageCount: 0 }, 'Fix login', 'codex')
    expect(none.message).toMatch(/already up to date/)
    expect(none.ok).toBe(true)
  })

  it('uses the singular for one new message', () => {
    const one = explainImportOutcome({ kind: 'refreshed', session, newMessageCount: 1 }, 'Fix login', 'codex')
    expect(one.message).toMatch(/1 new message to/)
    expect(one.message).not.toMatch(/1 new messages/)
  })

  it('never shows a raw outcome name to the person', () => {
    // "Could not import: corruptSession" was the headline for a damaged file.
    const bad = explainImportOutcome({ kind: 'corruptSession', detail: 'unexpected end of file' }, 'x', 'codex')
    expect(bad.ok).toBe(false)
    expect(bad.message).not.toMatch(/corruptSession/)
    expect(bad.message).toMatch(/unexpected end of file/)
  })

  it('names the client a person recognises when it is the client at fault', () => {
    const gone = explainImportOutcome({ kind: 'clientNotDetected' }, 'x', 'vscode-copilot')
    expect(gone.message).toMatch(/VS Code Copilot Chat/)
    expect(gone.message).not.toMatch(/vscode-copilot/)
  })
})

describe('explainImportScanRefusal', () => {
  it('names the folder it looked in, which is the fixable case', () => {
    const msg = explainImportScanRefusal({ kind: 'failed', error: { kind: 'missingPath', path: '/old/place' } })
    expect(msg).toMatch(/\/old\/place/)
    expect(msg).toMatch(/moved/i)
  })
  it('says which versions it can read', () => {
    const msg = explainImportScanRefusal({
      kind: 'failed',
      error: { kind: 'unsupportedVersion', found: '2.0', supportedRange: '1.x' },
    })
    expect(msg).toMatch(/2\.0/)
    expect(msg).toMatch(/1\.x/)
  })
  it('never states an absence it has not established', () => {
    // The old copy said "No sessions available right now" for every one of
    // these, which reads as "you have no chats there".
    for (const o of [
      { kind: 'adapterDisabled' },
      { kind: 'clientNotDetected' },
      { kind: 'failed', error: { kind: 'timedOut', millis: 5000 } },
    ] as const) {
      expect(explainImportScanRefusal(o)).not.toMatch(/no sessions/i)
    }
  })
})

describe('isUnresolvedProject', () => {
  // The backend builds this id when it cannot match an imported session's
  // project folder to a repo, and its comment says the UI is expected to show
  // a "project not found" state. Nothing checked the prefix, so the phrase
  // "Unresolved project" was rendered exactly like a real project name.
  it('recognises the synthetic id import uses when it cannot place a chat', () => {
    expect(isUnresolvedProject('unresolved:claude-code')).toBe(true)
    expect(isUnresolvedProject('unresolved:codex')).toBe(true)
  })

  it('leaves a real repo id alone', () => {
    expect(isUnresolvedProject('a1b2c3d4')).toBe(false)
    // A repo whose own name merely starts with the word must not be caught:
    // the prefix carries a colon precisely so it cannot collide.
    expect(isUnresolvedProject('unresolvedThings')).toBe(false)
  })

  it('is false rather than throwing when there is no id at all', () => {
    expect(isUnresolvedProject(null)).toBe(false)
    expect(isUnresolvedProject(undefined)).toBe(false)
  })
})

describe('every place that shows a project name checks whether it is real', () => {
  // A guard, not a unit test. `isUnresolvedProject` was added in pass 35 for
  // the context panel, and pass 36 found the sidebar row still printing the
  // stand-in phrase in ordinary grey -- the same fix, not applied to its
  // sibling, which is the shape four findings in a row have had.
  //
  // Verified in both directions: this fails if either renderer drops the
  // check (confirmed by removing it from each in turn), and it does not fire
  // on the workspace title bar, which shows the OPEN repo's name and has no
  // unresolved case to mark.
  const RENDERERS = [
    'components/domain/agent-desk/SessionContextPanel.tsx',
    'components/domain/agent-desk/SessionRow.tsx',
  ]

  it('marks an unplaced project everywhere a session header name is shown', async () => {
    // @ts-expect-error -- no @types/node in this project; available at runtime
    const { readFileSync } = await import('node:fs')
    // @ts-expect-error -- no @types/node in this project; available at runtime
    const { fileURLToPath } = await import('node:url')
    const root = fileURLToPath(new URL('../', import.meta.url))
    const missing = RENDERERS.filter((rel) => {
      const src = readFileSync(`${root}${rel}`, 'utf8')
      return src.includes('repoName') && !src.includes('isUnresolvedProject')
    })
    expect(missing, 'these show a project name without checking it is a real one').toEqual([])
  })
})

describe('bringing in many chats at once', () => {
  function item(
    id: string,
    outcome: BatchImportItem['outcome'],
    title = id
  ): BatchImportItem {
    return { externalSessionId: id, title, outcome }
  }
  const session = { header: { sessionId: 'gw-1' } } as never

  it('tells apart new chats, updated ones, and ones already up to date', () => {
    const summary = summarizeBatchImport(
      [
        item('a', { kind: 'created', session }),
        item('b', { kind: 'created', session }),
        item('c', { kind: 'refreshed', session, newMessageCount: 4 }),
        item('d', { kind: 'refreshed', session, newMessageCount: 0 }),
      ],
      'claude-code'
    )
    expect(summary).toMatchObject({ created: 2, updated: 1, unchanged: 1, ok: true })
    expect(summary.message).toBe('2 chats brought in - 1 updated - 1 already up to date')
  })

  /**
   * A press that did nothing still has to say so, or it reads as a click that
   * never registered -- Rule #1.
   */
  it('says something even when nothing changed', () => {
    const summary = summarizeBatchImport([], 'claude-code')
    expect(summary.message).toBe('Nothing to bring in')
  })

  /**
   * Refusals inside a batch are counted AND explained. A tally alone
   * ("3 could not be brought in") leaves nothing to act on.
   */
  it('carries a reason for every chat that refused', () => {
    const summary = summarizeBatchImport(
      [
        item('a', { kind: 'created', session }),
        item('b', { kind: 'corruptSession', detail: 'bad json at line 4' }, 'Broken chat'),
      ],
      'claude-code'
    )
    expect(summary.ok).toBe(false)
    expect(summary.failed).toHaveLength(1)
    expect(summary.failed[0]?.title).toBe('Broken chat')
    expect(summary.failed[0]?.reason).toContain('bad json at line 4')
    expect(summary.message).toContain('1 could not be brought in')
  })

  it('uses singular wording for a single chat', () => {
    const summary = summarizeBatchImport([item('a', { kind: 'created', session })], 'claude-code')
    expect(summary.message).toBe('1 chat brought in')
  })

  /**
   * Both numbers, not just "too many". The limit is what turns a refusal into
   * a step someone can take.
   */
  it('names how many were asked for and how many are allowed', () => {
    const message = explainBatchRefusal(
      { kind: 'tooMany', requested: 800, limit: 500 },
      'claude-code'
    )
    expect(message).toContain('800')
    expect(message).toContain('500')
    expect(message).toContain('Nothing was brought in')
  })

  it('names the tool by the name a person would recognise', () => {
    expect(explainBatchRefusal({ kind: 'clientNotDetected' }, 'vscode-copilot')).toContain(
      'VS Code Copilot Chat'
    )
  })
})

describe('keeping in sync', () => {
  /**
   * The toggle's copy is load-bearing. Turning it on makes GitWyrm read
   * another application's saved conversations on a timer and copy new ones in
   * with nobody present, so the label has to say that BEFORE the switch is
   * flipped -- not in a toast afterwards.
   */
  it('says chats will arrive on their own, and that it is off by default', () => {
    const copy = syncToggleCopy('Claude Code')
    expect(copy.description).toContain('Claude Code')
    expect(copy.description.toLowerCase()).toContain('on its own')
    expect(copy.description.toLowerCase()).toContain('off unless you turn it on')
  })

  it('reports what arrived on its own', () => {
    const line = explainSyncImport(
      { created: 3, updated: 1, unchanged: 0, failed: [], message: '', ok: true },
      'Codex'
    )
    expect(line).toContain('Codex')
    expect(line).toContain('3 new chats')
    expect(line).toContain('updated 1')
  })

  /** Nothing arrived, so nothing is announced -- a toast every few minutes
   * saying "no change" is noise, not feedback. */
  it('says nothing when nothing arrived', () => {
    expect(
      explainSyncImport(
        { created: 0, updated: 0, unchanged: 12, failed: [], message: '', ok: true },
        'Codex'
      )
    ).toBeNull()
  })

  it('checks in minutes, not seconds', () => {
    expect(SYNC_POLL_MS).toBeGreaterThanOrEqual(60_000)
  })
})
