import { describe, expect, it } from 'vitest'
import { blockedReason, usableProviders } from './ProviderControl'
import type { AgentProvider } from '@/lib/bindings'

const READY: AgentProvider = {
  id: 'copilot',
  displayName: 'GitHub Copilot',
  isDefault: true,
  installed: true,
  version: '1.0.80',
  tooOld: false,
  unresponsive: false,
  canDoReadOnlyWork: true,
  readOnlyLimit: null,
  homepageUrl: 'https://example.invalid/install',
  installHint: 'npm install -g example',
  binaryName: 'copilot',
  models: [],
  effortLevels: [],
}

describe('blockedReason', () => {
  it('lets an installed, capable tool through', () => {
    expect(blockedReason(READY, true)).toBeUndefined()
    expect(blockedReason(READY, false)).toBeUndefined()
  })

  it('says a missing tool is not installed, not that it is broken', () => {
    const reason = blockedReason({ ...READY, installed: false, version: null }, false)
    expect(reason).toBe('Not installed on this computer.')
  })

  it('tells the user an old tool can be fixed by updating it', () => {
    // "Too old" and "not installed" need different words: one asks for an
    // install, the other for an update. Collapsing them sends the user to do
    // the wrong thing.
    const reason = blockedReason({ ...READY, installed: false, tooOld: true, version: '0.9.0' }, false)
    expect(reason).toContain('0.9.0')
    expect(reason).toContain('Updating')
  })

  it('blocks a tool that cannot promise read-only, but only for read-only work', () => {
    const opencode: AgentProvider = {
      ...READY,
      id: 'opencode',
      displayName: 'opencode',
      isDefault: false,
      canDoReadOnlyWork: false,
      readOnlyLimit: 'opencode has no way to be told to leave your files alone.',
    }
    expect(blockedReason(opencode, true)).toContain('opencode')
    // The same tool is perfectly usable where writing is the point.
    expect(blockedReason(opencode, false)).toBeUndefined()
  })

  it('reports a missing install before a read-only limit', () => {
    // Both are true at once for an uninstalled tool that also cannot promise
    // read-only. Installing it is the actionable step, so that is the one to
    // name; leading with the capability limit would read as "do not bother".
    const both: AgentProvider = {
      ...READY,
      installed: false,
      version: null,
      canDoReadOnlyWork: false,
      readOnlyLimit: 'cannot be told to leave your files alone',
    }
    expect(blockedReason(both, true)).toBe('Not installed on this computer.')
  })

  it('falls back to its own words when the backend sent no explanation', () => {
    const noText: AgentProvider = { ...READY, canDoReadOnlyWork: false, readOnlyLimit: null }
    expect(blockedReason(noText, true)).toBeTruthy()
  })
})

describe('blockedReason for a tool that did not answer', () => {
  it('does not tell you to install something already on the machine', () => {
    // The probe reports `installed: false` for an unresponsive tool, so
    // without its own branch this said "Not installed on this computer."
    // about a binary the person can see in their terminal.
    const reason = blockedReason({ ...READY, installed: false, unresponsive: true }, false)
    expect(reason).toMatch(/did not answer/i)
    expect(reason).not.toMatch(/not installed/i)
  })

  it('still says not installed when it genuinely is not there', () => {
    expect(blockedReason({ ...READY, installed: false, unresponsive: false }, false)).toMatch(/not installed/i)
  })

  it('lets too-old win, since that has a specific fix', () => {
    expect(blockedReason({ ...READY, installed: false, tooOld: true, unresponsive: true }, false)).toMatch(/too old/i)
  })
})

describe('usableProviders', () => {
  it('keeps only the tools this chat can actually run with', () => {
    const missing = { ...READY, id: 'gemini', displayName: 'Gemini CLI', installed: false, version: null }
    const old = { ...READY, id: 'codex', displayName: 'Codex', tooOld: true, version: '0.1.0' }
    const kept = usableProviders([READY, missing, old], false)
    expect(kept.map((r) => r.id)).toEqual(['copilot'])
  })

  it('drops a tool that cannot be trusted with a read-only chat, but only there', () => {
    // opencode has no way to be told to leave files alone, so it is a real
    // choice for a chat that may write and no choice at all for one that
    // must not -- the same tool, filtered differently by the chat it is for.
    const opencode = {
      ...READY,
      id: 'opencode',
      displayName: 'opencode',
      isDefault: false,
      canDoReadOnlyWork: false,
      readOnlyLimit: 'opencode has no way to be told to leave your files alone.',
    }
    expect(usableProviders([READY, opencode], true).map((r) => r.id)).toEqual(['copilot'])
    expect(usableProviders([READY, opencode], false).map((r) => r.id)).toEqual(['copilot', 'opencode'])
  })
})
