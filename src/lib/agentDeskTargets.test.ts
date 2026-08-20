import { describe, expect, it } from 'vitest'
import type { MessageTarget } from '@/lib/bindings'
import { resolveMessageTarget } from './agentDeskTargets'

describe('resolveMessageTarget', () => {
  it('resolves source targets to a reachable destination', () => {
    const target: MessageTarget = { kind: 'source' }
    const resolved = resolveMessageTarget(target)
    expect(resolved.kind).toBe('source')
  })

  it('marks file targets as unavailable with the path as the label', () => {
    const target: MessageTarget = { kind: 'file', path: 'src/lib/foo.ts' }
    const resolved = resolveMessageTarget(target)
    expect(resolved.kind).toBe('unavailable')
    expect(resolved.label).toBe('src/lib/foo.ts')
    if (resolved.kind !== 'unavailable') throw new Error('expected unavailable')
    expect(resolved.reason.length).toBeGreaterThan(0)
  })

  it('marks diff targets as unavailable, falling back to a generic label when scope is empty', () => {
    const withScope = resolveMessageTarget({ kind: 'diff', scope: 'src/**' })
    expect(withScope.kind).toBe('unavailable')
    expect(withScope.label).toBe('src/**')

    const withoutScope = resolveMessageTarget({ kind: 'diff', scope: '' })
    expect(withoutScope.kind).toBe('unavailable')
    expect(withoutScope.label).toBe('View diff')
  })

  it('marks graphNode targets as unavailable', () => {
    const target: MessageTarget = { kind: 'graphNode', executionId: 'exec-1' }
    const resolved = resolveMessageTarget(target)
    expect(resolved.kind).toBe('unavailable')
  })

  it('marks openSpecTask targets as unavailable with a 1-based task label', () => {
    const target: MessageTarget = { kind: 'openSpecTask', changeId: 'change-1', taskIndex: 2 }
    const resolved = resolveMessageTarget(target)
    expect(resolved.kind).toBe('unavailable')
    expect(resolved.label).toBe('Task 3')
  })

  it('never returns an empty reason for an unavailable target', () => {
    const targets: MessageTarget[] = [
      { kind: 'file', path: 'a.ts' },
      { kind: 'diff', scope: 'a' },
      { kind: 'graphNode', executionId: 'e' },
      { kind: 'openSpecTask', changeId: 'c', taskIndex: 0 },
    ]
    for (const target of targets) {
      const resolved = resolveMessageTarget(target)
      if (resolved.kind === 'unavailable') {
        expect(resolved.reason.trim().length).toBeGreaterThan(0)
      }
    }
  })
})
