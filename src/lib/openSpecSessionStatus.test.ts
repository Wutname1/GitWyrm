import { describe, expect, it } from 'vitest'
import type { OpenSpecSessionStatus } from '@/lib/bindings'
import { openSpecStatusLine } from './openSpecSessionStatus'

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
