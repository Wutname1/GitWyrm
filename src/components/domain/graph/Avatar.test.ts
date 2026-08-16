import { describe, it, expect } from 'vitest'
import { isDependabot } from './Avatar'

describe('isDependabot', () => {
  it('matches the address dependabot actually commits under', () => {
    expect(isDependabot('49699333+dependabot[bot]@users.noreply.github.com')).toBe(true)
  })

  it('matches the older forms without the id or the [bot] suffix', () => {
    expect(isDependabot('dependabot[bot]@users.noreply.github.com')).toBe(true)
    expect(isDependabot('dependabot@users.noreply.github.com')).toBe(true)
  })

  it('ignores surrounding space and case', () => {
    expect(isDependabot('  49699333+Dependabot[BOT]@users.noreply.github.com ')).toBe(true)
  })

  it('leaves people alone, including lookalike logins', () => {
    expect(isDependabot('1234+octocat@users.noreply.github.com')).toBe(false)
    expect(isDependabot('notdependabot@users.noreply.github.com')).toBe(false)
    expect(isDependabot('dependabot@example.com')).toBe(false)
    expect(isDependabot('')).toBe(false)
  })
})
