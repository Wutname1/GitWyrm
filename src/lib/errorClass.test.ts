import { describe, expect, it } from 'vitest'
import { classifyError } from './errorClass'

/**
 * Push used to fail outright on a branch that had never been published, with
 * git's own "The current branch master has no upstream branch" and a command
 * for the user to type. Push now publishes such a branch itself, so these
 * classifications cover what is left: the setup situations the app genuinely
 * cannot decide on the user's behalf.
 */
describe('a push with nowhere to go', () => {
  it('treats a project with no remote as a setup step, not a failure', () => {
    const { severity, message } = classifyError(
      new Error('This repository has no remote to push to.')
    )
    expect(severity).toBe('warning')
    expect(message).toMatch(/no cloud copy yet/i)
    expect(message).toMatch(/Remotes/)
  })

  it('asks which remote when several are set up and none is origin', () => {
    const { severity, message } = classifyError(
      new Error('Several remotes are set up. Pick one in Remotes first.')
    )
    expect(severity).toBe('warning')
    expect(message).toMatch(/more than one cloud copy/i)
  })

  /**
   * Safety net for any push path that still reaches git's raw complaint. The
   * user must never be shown a command to type -- that is the failure mode this
   * whole change removes.
   */
  it('never echoes gits type-this-command advice back at the user', () => {
    const { severity, message } = classifyError(
      new Error(
        'git push failed: fatal: The current branch master has no upstream branch.\n' +
          'To push the current branch and set the remote as upstream, use\n' +
          '    git push --set-upstream origin master'
      )
    )
    expect(severity).toBe('warning')
    expect(message).not.toMatch(/--set-upstream/)
    expect(message).not.toMatch(/git push/)
    expect(message).toMatch(/isn't on the cloud yet/i)
  })

  /** The original text is always kept for the log, whatever the user is shown. */
  it('keeps the raw message for the log', () => {
    const raw = 'This repository has no remote to push to.'
    expect(classifyError(new Error(raw)).raw).toContain(raw)
  })
})
