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

describe('an operation refused because the working tree is dirty', () => {
  // Every site raises the same opening and varies the tail with the operation.
  const RAWS = [
    'working tree has changes; commit or stash before merging',
    'working tree has changes; commit or stash before switching branches',
    'working tree has changes; commit or stash before cherry-picking',
    'working tree has changes; commit or stash before rewriting history',
  ]

  it('is a warning for every operation, so a deliberate refusal is never a crash report', () => {
    for (const raw of RAWS) {
      expect(classifyError(new Error(raw)).severity).toBe('warning')
    }
  })

  it('says what to do instead of repeating the backend sentence', () => {
    const { message } = classifyError(new Error(RAWS[0]))
    expect(message).toBe(
      'You have changes that this would overwrite. Commit or stash them first, then try again.',
    )
    expect(message).not.toMatch(/working tree/i)
  })
})

describe('a remote delete that reported success but changed nothing', () => {
  const RAW =
    'feature-x is still on origin. The delete reported success but the branch is still there.'

  it('tells the user the branch survived, without git jargon', () => {
    const { message } = classifyError(new Error(RAW))
    expect(message).toMatch(/still has that branch/i)
    expect(message).not.toMatch(/refs\//)
  })

  it('is a warning: the host accepted it, so there is nothing local to fix', () => {
    expect(classifyError(new Error(RAW)).severity).toBe('warning')
  })
})

describe('a pull with nothing linked to pull from', () => {
  // Verbatim from GITWYRM-BACKEND-7.
  const RAW =
    'mutation failed [error]: git pull failed: There is no tracking information for the current branch.'

  it('explains the branch is unlinked instead of echoing git', () => {
    const { message } = classifyError(new Error(RAW))
    expect(message).toMatch(/isn't linked to a cloud copy/i)
    expect(message).not.toMatch(/tracking information/i)
  })

  it('is a warning, matching how the backend already classifies it', () => {
    expect(classifyError(new Error(RAW)).severity).toBe('warning')
  })

  it('treats a diverged pull the same way', () => {
    const raw =
      'git pull failed: You have divergent branches and need to specify how to reconcile them.'
    const { severity, message } = classifyError(new Error(raw))
    expect(severity).toBe('warning')
    expect(message).toMatch(/merge or rebase/i)
  })
})

describe('a cloud copy the host will not admit exists', () => {
  // Verbatim from GITWYRM-BACKEND-2.
  const RAW =
    "Command failed: git push failed: fatal: repository 'https://github.com/owner/repo.git/' not found"

  it('names every reason it could be, since the host will not say which', () => {
    const { message } = classifyError(new Error(RAW))
    expect(message).toMatch(/renamed or deleted/i)
    expect(message).not.toMatch(/fatal:/i)
  })

  it('is a warning: nothing local can fix a repo the host denies', () => {
    expect(classifyError(new Error(RAW)).severity).toBe('warning')
  })

  it('covers the API wording for the same condition', () => {
    const raw = 'GitHub could not find that. It may be private, renamed, or your token may not cover it.'
    expect(classifyError(new Error(raw)).severity).toBe('warning')
  })

  it('covers the fetch path wording, which the backend classified but this did not', () => {
    // Verbatim from GITWYRM-BACKEND-2, which filed 114 crash reports because
    // this sentence is built separately in commands/remote.rs and matched none
    // of the needles above.
    const raw =
      'Command failed: git clone failed: Could not find https://github.com/owner/repo with your sign-in. It may have been moved or renamed, or your account may not have access to it.'
    const { severity, message } = classifyError(new Error(raw))
    expect(severity).toBe('warning')
    expect(message).toMatch(/renamed or deleted/i)
  })

  it('does not reach a genuine object lookup failure', () => {
    // "could not find" alone would swallow these, which are real faults that
    // must keep reporting.
    for (const raw of [
      'git error: could not find commit 4f2b1a9; class=Odb (9); code=NotFound (-3)',
      'could not find object in database',
    ]) {
      expect(classifyError(new Error(raw)).severity).toBe('error')
    }
  })
})

describe('picking lines whose diff has already moved on', () => {
  // Verbatim from GITWYRM-BACKEND-6. The backend classified this as expected in
  // 1a3cac4, but no rule here matched it, so the frontend kept filing crashes.
  const RAW = 'mutation failed [error]: no changes found for this file'

  it('is a refusal, not a fault', () => {
    expect(classifyError(new Error(RAW)).severity).toBe('info')
  })

  it('tells the user what to do instead of repeating the backend wording', () => {
    const { message } = classifyError(new Error(RAW))
    expect(message).toMatch(/already moved on/i)
    expect(message).not.toMatch(/mutation failed/i)
  })

  it('does not reach a genuine diff failure that merely mentions changes', () => {
    const raw = 'git error: failed to load changes for this file; class=Diff (20)'
    expect(classifyError(new Error(raw)).severity).toBe('error')
  })
})

describe('an index that is mid-conflict or damaged', () => {
  it('treats a half-resolved merge as the conflict doing its job', () => {
    const raw =
      'git error: cannot create a tree from a not fully merged index.; class=Index (10); code=Unmerged (-10)'
    const { severity, message } = classifyError(new Error(raw))
    expect(severity).toBe('warning')
    expect(message).toMatch(/conflicts still need resolving/i)
  })

  it('keeps a corrupt index at error severity, so it never stops reporting', () => {
    const raws = [
      'git error: invalid data in index - incorrect header signature; class=Index (10)',
      'git fetch failed: fatal: index file corrupt',
    ]
    for (const raw of raws) {
      expect(classifyError(new Error(raw)).severity).toBe('error')
    }
  })

  it('reassures that commits survive a damaged index', () => {
    const { message } = classifyError(
      new Error('git error: invalid data in index - incorrect header signature; class=Index (10)')
    )
    expect(message).toMatch(/commits are safe/i)
    expect(message).not.toMatch(/header signature/i)
  })
})

describe('a remote that has no credentials yet', () => {
  // Verbatim from GITWYRM-BACKEND-4.
  const RAW =
    'Command failed: git fetch failed: Sign-in needed for https://github.com. Connect the account, then try again.'

  it('is a warning, not a crash: nothing is broken yet', () => {
    expect(classifyError(new Error(RAW)).severity).toBe('warning')
  })

  it('wins over the generic auth rule, which would call it an error', () => {
    const { severity, message } = classifyError(new Error(RAW))
    expect(severity).not.toBe('error')
    expect(message).toMatch(/connect your account/i)
  })

  it('leaves a genuinely rejected credential as an error', () => {
    const raw = 'git error: authentication failed for https://github.com'
    expect(classifyError(new Error(raw)).severity).toBe('error')
  })
})

describe('a host that was never connected', () => {
  // Verbatim from GITWYRM-BACKEND-4.
  const RAW = 'Command failed: not signed in to GitHub; connect GitHub first'

  it('is a warning: connecting an account is setup, not a fault', () => {
    expect(classifyError(new Error(RAW)).severity).toBe('warning')
  })

  it('points at Settings instead of repeating the backend sentence', () => {
    const { message } = classifyError(new Error(RAW))
    expect(message).toMatch(/not connected yet/i)
    expect(message).toMatch(/Integrations/)
  })

  it('carries a button to the Integrations page', () => {
    expect(classifyError(new Error(RAW)).fix?.section).toBe('integrations')
  })

  it('covers every host the helper phrases, not just GitHub', () => {
    for (const host of ['GitLab', 'Bitbucket', 'Azure DevOps']) {
      const raw = `not signed in to ${host}; connect ${host} first`
      expect(classifyError(new Error(raw)).severity).toBe('warning')
    }
  })

  it('still reports a real authentication failure as an error', () => {
    expect(classifyError(new Error('git error: authentication failed')).severity).toBe('error')
  })
})

describe('a cloud branch name given to a local-only command', () => {
  // reject_remote_qualified's wording, from GITWYRM-BACKEND-2/6.
  const RAW = "'origin/development' is a branch on the remote. To link the local copy, use 'development'."

  it('is a warning: the guard refusing is not a fault', () => {
    expect(classifyError(new Error(RAW)).severity).toBe('warning')
  })

  it('explains which copy to use without git wording', () => {
    const { message } = classifyError(new Error(RAW))
    expect(message).toMatch(/cloud copy/i)
    expect(message).not.toMatch(/refs\/|libgit2/i)
  })

  it('still reports the raw libgit2 failure it replaced', () => {
    const raw = "git error: cannot locate local branch 'origin/development'; class=Reference (4)"
    expect(classifyError(new Error(raw)).severity).toBe('error')
  })
})

describe('a push the server refused after accepting it', () => {
  // Verbatim from a HearthShelf-Mobile push, 2026-09-22.
  const WORKFLOW =
    'git push failed: ! [remote rejected] main -> main (refusing to allow an OAuth App to create or update workflow `.github/workflows/release.yml` without `workflow` scope)'

  it('does not tell the user the cloud is ahead when it is not', () => {
    expect(classifyError(new Error(WORKFLOW)).message).not.toMatch(/cloud has changes/i)
  })

  it('names the missing workflow permission and how to grant it', () => {
    const { severity, message } = classifyError(new Error(WORKFLOW))
    expect(severity).toBe('warning')
    expect(message).toMatch(/\.github\/workflows\/release\.yml/)
    expect(message).toMatch(/not affected/i)
    expect(message).toMatch(/reconnect github/i)
  })

  it('takes the user to the GitHub account row to reconnect', () => {
    expect(classifyError(new Error(WORKFLOW)).fix).toEqual({
      label: 'Reconnect GitHub',
      section: 'integrations',
      settingId: 'github-connection',
    })
  })

  it('offers no settings button for a refusal settings cannot fix', () => {
    const raw = 'git push failed: ! [remote rejected] main -> main (pre-receive hook declined)'
    expect(classifyError(new Error(raw)).fix).toBeUndefined()
  })

  it('passes on the server reason for other refusals', () => {
    const raw = 'git push failed: ! [remote rejected] main -> main (pre-receive hook declined)'
    const { severity, message } = classifyError(new Error(raw))
    expect(severity).toBe('warning')
    expect(message).toBe('The cloud refused this push. Its reason: pre-receive hook declined')
  })

  it('still reads a real non-fast-forward as the cloud being ahead', () => {
    const raw = 'git push failed: ! [rejected]        main -> main (fetch first)'
    expect(classifyError(new Error(raw)).message).toMatch(/cloud has changes/i)
  })
})
