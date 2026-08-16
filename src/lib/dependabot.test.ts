import { describe, expect, it } from 'vitest'
import {
  DEPENDABOT_COMMANDS,
  DEPENDABOT_IGNORE_COMMANDS,
  isDependabot,
} from './dependabot'

describe('isDependabot', () => {
  it('matches the bot account', () => {
    expect(isDependabot('dependabot[bot]')).toBe(true)
  })

  it('ignores case and surrounding space', () => {
    expect(isDependabot('  Dependabot[bot] ')).toBe(true)
  })

  it('does not match a human who picked a similar name', () => {
    // The `[bot]` suffix is the part that cannot be claimed by a user account.
    expect(isDependabot('dependabot')).toBe(false)
    expect(isDependabot('not-dependabot[bot]')).toBe(false)
  })

  it('does not match other bots', () => {
    expect(isDependabot('renovate[bot]')).toBe(false)
  })
})

describe('command list', () => {
  /**
   * merge, squash and merge, cancel merge, close and reopen were removed by
   * GitHub on 2026-01-27. Posting one is a no-op, so a button for it would be a
   * control that silently does nothing.
   */
  it('offers no command that GitHub has removed', () => {
    const removed = [
      '@dependabot merge',
      '@dependabot squash and merge',
      '@dependabot cancel merge',
      '@dependabot close',
      '@dependabot reopen',
    ]
    const all = [...DEPENDABOT_COMMANDS, ...DEPENDABOT_IGNORE_COMMANDS]
    for (const cmd of all) {
      expect(removed).not.toContain(cmd.command)
    }
  })

  it('addresses every command to dependabot', () => {
    const all = [...DEPENDABOT_COMMANDS, ...DEPENDABOT_IGNORE_COMMANDS]
    for (const cmd of all) {
      expect(cmd.command.startsWith('@dependabot ')).toBe(true)
    }
  })

  it('marks every ignore command destructive, since each one closes the PR', () => {
    for (const cmd of DEPENDABOT_IGNORE_COMMANDS) {
      expect(cmd.destructive).toBe(true)
    }
  })

  it('uses unique ids so the pending button can be told apart', () => {
    const ids = [...DEPENDABOT_COMMANDS, ...DEPENDABOT_IGNORE_COMMANDS].map((c) => c.id)
    expect(new Set(ids).size).toBe(ids.length)
  })
})
