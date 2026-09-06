import { describe, expect, it } from 'vitest'
import { aiChipNoun, aiChipOffDescription, aiChipScopeNote, type AiChipScope } from './aiChipScope'

const SCOPES: AiChipScope[] = ['all', 'writing']

describe('aiChipScope', () => {
  /**
   * The defect: in Agent Desk the chip called itself the trust anchor for
   * every AI action in the window, while a chat below it ran on a provider
   * it had never heard of. In that window it must name only what it governs.
   */
  it('names only writing help where chats have their own AI', () => {
    expect(aiChipNoun('writing')).toBe('Writing help')
  })

  /** Spec Desk has one AI, so there the chip really does speak for all of it. */
  it('stays the plain AI label where it is the only AI', () => {
    expect(aiChipNoun('all')).toBe('AI')
  })

  it('points at the per-chat control wherever chats have their own AI', () => {
    expect(aiChipScopeNote('writing')).toContain('Each chat picks its own AI')
  })

  it('adds nothing where there is no second AI to distinguish it from', () => {
    expect(aiChipScopeNote('all')).toBe('')
  })

  /**
   * Turning writing help off must not read as turning chats off. The old
   * copy listed what survived and left chats out of the list.
   */
  it('says plainly that switching off leaves chats alone', () => {
    expect(aiChipOffDescription('writing', 'Claude')).toContain('chats are not affected')
  })

  it('keeps the sign-in reassurance in both windows', () => {
    for (const scope of SCOPES) {
      expect(aiChipOffDescription(scope, 'Claude')).toContain('Claude stays signed in')
    }
  })

  /**
   * Rule #2: this copy is read by someone who does not know what a provider
   * is, so it must not name one.
   */
  it('uses no jargon a beginner would have to look up', () => {
    const jargon = ['provider', 'backend', 'ACP', 'adapter', 'session']
    const strings = SCOPES.flatMap((s) => [
      aiChipNoun(s),
      aiChipScopeNote(s),
      aiChipOffDescription(s, 'Claude'),
    ])
    for (const text of strings) {
      for (const word of jargon) {
        expect(text.toLowerCase()).not.toContain(word.toLowerCase())
      }
    }
  })
})
