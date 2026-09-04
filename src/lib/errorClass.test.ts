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

describe('a backend message carrying diagnostics', () => {
  // What the commit path now sends: one actionable sentence, then the raw gpg
  // output behind the separator.
  const SEPARATOR = '\n␞\n'
  const HINT =
    'The signing key this repository uses is missing. Pick a different key in Settings > Security, or turn signing off.'
  const DETAIL = [
    'error: gpg failed to sign the data:',
    'gpg: skipped "32BD8D9B66ABAD8B": Input/output error',
    '[GNUPG:] INV_SGNR 0 32BD8D9B66ABAD8B',
  ].join('\n')

  it('shows only the sentence, not the gpg wall', () => {
    const { message } = classifyError(new Error(`${HINT}${SEPARATOR}${DETAIL}`))
    expect(message).toBe(HINT)
    expect(message).not.toMatch(/gpg:/)
    expect(message).not.toMatch(/GNUPG/)
  })

  it('keeps the diagnostics on raw for the log and the copy button', () => {
    const { raw } = classifyError(new Error(`${HINT}${SEPARATOR}${DETAIL}`))
    expect(raw).toContain('INV_SGNR')
    expect(raw).toContain('32BD8D9B66ABAD8B')
    // The separator itself is an internal marker and should never be shown.
    expect(raw).not.toContain('␞')
  })

  it('leaves an ordinary message alone', () => {
    const { message } = classifyError(new Error('Something ordinary broke.'))
    expect(message).toBe('Something ordinary broke.')
  })
})

/**
 * The backend already lists this refusal as expected and stays quiet about it,
 * but the frontend had no matching rule -- so it logged raw libgit2 wording at
 * error severity and filed a crash report for a routine "that branch is open
 * elsewhere". The two layers have to agree.
 */
describe('a branch held by another worktree', () => {
  const RAW =
    "git error: cannot set HEAD to reference 'refs/heads/main' as it is the current HEAD of a linked repository.; class=Repository (6)"

  it('explains itself without libgit2 jargon', () => {
    const { message } = classifyError(new Error(RAW))
    expect(message).toBe(
      'That branch is already open in another worktree. A branch can only be checked out in one folder at a time.',
    )
    expect(message).not.toMatch(/HEAD/)
    expect(message).not.toMatch(/class=/)
  })

  it('is a warning, so it never becomes a crash report', () => {
    expect(classifyError(new Error(RAW)).severity).toBe('warning')
  })
})

/**
 * A terminal open beside the app is enough to cause this, so it is routine
 * rather than a fault. The backend lists it as expected; the frontend needs the
 * matching rule or the two layers disagree and it is filed as a crash.
 */
describe('another program holding the index', () => {
  const RAW =
    'git error: the index is locked; this might be due to a concurrent or crashed process; class=Index (10); code=Locked (-14)'

  it('says what to do without git jargon', () => {
    const { message } = classifyError(new Error(RAW))
    expect(message).toBe(
      'Another program is using this repository right now. Wait for it to finish, then try again.',
    )
    expect(message).not.toMatch(/index/i)
    expect(message).not.toMatch(/class=/)
  })

  it('is a warning, so it never becomes a crash report', () => {
    expect(classifyError(new Error(RAW)).severity).toBe('warning')
  })
})
