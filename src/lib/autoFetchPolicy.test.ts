import { describe, expect, it } from 'vitest'
import {
  BACKGROUND_FETCH_INTERVAL_MS,
  BACKGROUND_FETCH_STAGGER_MS,
  SWEEP_BUDGET,
  isAuthFailure,
  staggerFor,
} from './autoFetchPolicy'

/**
 * These guard the auto-fetch back-off added after a field log showed the sweep
 * retrying an unauthorised remote every ~20 seconds across 22 repositories.
 * Every one of those attempts ran the credential helper, which is what
 * "spammed with login prompts" looked like from inside the app.
 */
describe('isAuthFailure', () => {
  /**
   * The exact strings seen in production logs and Sentry. These are the whole
   * reason the back-off exists, so they are pinned verbatim.
   */
  it('recognises the failures that actually reached users', () => {
    for (const message of [
      'git fetch failed: Sign-in needed for https://github.com. Connect the account, then try again.',
      "fatal: could not read Username for 'https://github.com': No such file or directory",
      "fatal: could not read Username for 'https://github.com': terminal prompts disabled",
      'GitHub refused: the `comcast-mgee` organization has enabled OAuth App access restrictions',
      'gh: Resource protected by organization SAML enforcement. (HTTP 403)',
      "fatal: Authentication failed for 'https://github.com'",
      'git fetch failed: Could not find https://github.com/org/OldName.git with your sign-in. It may have been moved or renamed, or your account may not have access to it.',
    ]) {
      expect(isAuthFailure(message), message).toBe(true)
    }
  })

  /**
   * The dangerous direction. Pausing a healthy repo because of a network blip
   * would silently stop updating it for the session -- far worse than one
   * wasted retry -- so anything transient must NOT match.
   */
  it('leaves transient and unrelated failures alone', () => {
    for (const message of [
      'git fetch failed: could not resolve host: github.com',
      'git fetch failed: failed to connect to github.com port 443: Timed out',
      'git push failed: [rejected] main -> main (fetch first)',
      'git fetch failed: cannot lock ref refs/remotes/origin/main',
      'git pull failed: merge conflicts',
      'no remote configured',
      '',
    ]) {
      expect(isAuthFailure(message), message).toBe(false)
    }
  })

  /**
   * A bare "403" substring would match a branch or path containing those
   * digits, pausing a repo that is perfectly healthy.
   */
  it('does not treat an incidental 403 in a ref name as an auth failure', () => {
    expect(isAuthFailure('git fetch failed: cannot lock ref refs/heads/fix-403-error')).toBe(false)
    expect(isAuthFailure('git push failed: branch bug/403-handling rejected')).toBe(false)
  })

  it('is case insensitive, since messages come from several sources', () => {
    expect(isAuthFailure('SIGN-IN NEEDED for https://github.com')).toBe(true)
    expect(isAuthFailure('Could Not Read Username')).toBe(true)
  })
})

describe('staggerFor', () => {
  /**
   * The 22-repo case from the field log: the compressed gap is what produced a
   * fetch roughly every 20 seconds overall, making one lapsed authorization
   * look like a continuous storm.
   */
  it('compresses the gap as repo count grows', () => {
    expect(staggerFor(1)).toBe(BACKGROUND_FETCH_STAGGER_MS)
    const many = staggerFor(22)
    expect(many).toBeLessThanOrEqual(BACKGROUND_FETCH_STAGGER_MS)
    expect(many).toBeGreaterThan(0)
  })

  it('never schedules a sweep past its own interval', () => {
    for (const count of [2, 10, 22, 50, 200]) {
      const total = staggerFor(count) * (count - 1)
      expect(total, `${count} repos`).toBeLessThanOrEqual(
        BACKGROUND_FETCH_INTERVAL_MS * SWEEP_BUDGET
      )
    }
  })
})
