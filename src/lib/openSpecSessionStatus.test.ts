import { describe, expect, it } from 'vitest'
import type { OpenSpecSessionStatus } from '@/lib/bindings'
import { explainDriftUnavailable, openSpecProgressLine, openSpecStatusLine } from './openSpecSessionStatus'

describe('openSpecStatusLine', () => {
  it('shows nothing for an undefined status (still loading)', () => {
    expect(openSpecStatusLine(undefined)).toBeNull()
  })

  it('shows nothing when the change is active -- the generic banner covers that', () => {
    const status: OpenSpecSessionStatus = { kind: 'active' }
    expect(openSpecStatusLine(status)).toBeNull()
  })

  it('shows nothing when the session source is not OpenSpec at all', () => {
    const status: OpenSpecSessionStatus = { kind: 'notAnOpenSpecSource' }
    expect(openSpecStatusLine(status)).toBeNull()
  })

  it('reports an archived change as a neutral, normal end state', () => {
    const status: OpenSpecSessionStatus = { kind: 'archived' }
    const line = openSpecStatusLine(status)
    expect(line).not.toBeNull()
    expect(line?.tone).toBe('neutral')
    expect(line?.text).toContain('archived')
  })

  it('names the likely new id for a moved, still-active change', () => {
    const status: OpenSpecSessionStatus = { kind: 'moved', likelyNewId: 'add-thing-v2', archived: false }
    const line = openSpecStatusLine(status)
    expect(line?.tone).toBe('amber')
    expect(line?.text).toContain('add-thing-v2')
    expect(line?.text).not.toContain('archived')
  })

  it('names the likely new id and mentions archived for a moved-and-archived change', () => {
    const status: OpenSpecSessionStatus = { kind: 'moved', likelyNewId: 'add-thing-v2', archived: true }
    const line = openSpecStatusLine(status)
    expect(line?.tone).toBe('amber')
    expect(line?.text).toContain('add-thing-v2')
    expect(line?.text).toContain('archived')
  })

  it('reports a deleted change with an amber, actionable tone', () => {
    const status: OpenSpecSessionStatus = { kind: 'deleted' }
    const line = openSpecStatusLine(status)
    expect(line?.tone).toBe('amber')
    expect(line?.text.length).toBeGreaterThan(0)
  })

  it('tells the user to open the repository when it is not open', () => {
    const status: OpenSpecSessionStatus = { kind: 'repoNotOpen' }
    const line = openSpecStatusLine(status)
    expect(line?.tone).toBe('neutral')
    expect(line?.text.toLowerCase()).toContain('repository')
  })

  it('shows nothing for session-level failures -- the generic banner already covers those', () => {
    expect(openSpecStatusLine({ kind: 'sessionNotFound' })).toBeNull()
    expect(openSpecStatusLine({ kind: 'sessionDamaged', reason: 'bad json' })).toBeNull()
    expect(openSpecStatusLine({ kind: 'sessionUnavailable', detail: 'locked' })).toBeNull()
  })
})

describe('openSpecProgressLine', () => {
  const ctx = (progress: { done: number; total: number; percent: number; is_draft: boolean }) =>
    ({ kind: 'found', value: { progress } }) as unknown as Parameters<typeof openSpecProgressLine>[0]

  it('says how many tasks are done', () => {
    expect(openSpecProgressLine(ctx({ done: 3, total: 8, percent: 38, is_draft: false }))).toBe('3 of 8 tasks done')
  })
  it('says so plainly when everything is done', () => {
    expect(openSpecProgressLine(ctx({ done: 8, total: 8, percent: 100, is_draft: false }))).toBe('All 8 tasks done')
  })
  it('uses the singular for a one-task change, which is the common case', () => {
    // "All 1 tasks done" was the old wording, on the surface the vision calls
    // the defining advantage.
    expect(openSpecProgressLine(ctx({ done: 1, total: 1, percent: 100, is_draft: false }))).toBe('The one task is done')
    expect(openSpecProgressLine(ctx({ done: 0, total: 1, percent: 0, is_draft: false }))).toBe('0 of 1 task done')
  })
  it('calls a change with no tasks a draft, not 0%', () => {
    expect(openSpecProgressLine(ctx({ done: 0, total: 0, percent: 0, is_draft: true }))).toBe('No tasks written yet')
  })
  it('stays silent when the change could not be read', () => {
    // Unknown must stay unknown -- never rendered as zero progress.
    for (const kind of ['repoNotOpen', 'noOpenSpecFolder', 'sessionNotFound', 'notAnOpenSpecSource'] as const) {
      expect(openSpecProgressLine({ kind } as Parameters<typeof openSpecProgressLine>[0])).toBeNull()
    }
    expect(openSpecProgressLine(undefined)).toBeNull()
  })
})

describe('explainDriftUnavailable', () => {
  it('says when the check could not run, rather than implying nothing changed', () => {
    expect(explainDriftUnavailable({ kind: 'repoNotOpen' }, false)).toMatch(/cannot tell/i)
    expect(explainDriftUnavailable({ kind: 'sessionDamaged', reason: 'bad json' }, false)).toMatch(/bad json/)
    expect(explainDriftUnavailable(undefined, true)).toMatch(/could not check/i)
  })
  it('stays silent when there is a real answer', () => {
    // `checked` is read directly by the caller; `nothingToCompare` is not a
    // failure, so neither should raise an alarm.
    expect(explainDriftUnavailable({ kind: 'checked', diverged: false, launched_at: '' }, false)).toBeNull()
    expect(explainDriftUnavailable({ kind: 'nothingToCompare' }, false)).toBeNull()
    expect(explainDriftUnavailable(undefined, false)).toBeNull()
  })
})

describe('the archived line is reachable', () => {
  // It was not. The panel only asked for this status when the session's
  // snapshot already said the source was unavailable -- and the backend
  // deliberately keeps that flag false for an archived change, calling it "a
  // normal end state, not an outage". So the one case this function was
  // written for was the one case it was never asked about, and a chat bound
  // to a finished change just showed "8 of 8 tasks done" with no sign the
  // change had been archived out from under it.
  it('says a change is finished and archived', () => {
    const line = openSpecStatusLine({ kind: 'archived' })
    expect(line?.text).toMatch(/archived/i)
  })

  // The property that makes asking always safe: an active change contributes
  // no line, so enabling the query everywhere cannot add noise.
  it('says nothing at all for a change that is still active', () => {
    expect(openSpecStatusLine({ kind: 'active' })).toBeNull()
  })
})
