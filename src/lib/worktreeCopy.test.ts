import { describe, expect, it } from 'vitest'
import type { DirtyCount, Worktree } from '@/lib/bindings'
import {
  branchCleanupOffer,
  branchHeldMessage,
  brokenAction,
  brokenExplanation,
  dirtySummary,
  hasUnrecoverableWork,
  linkedWorktrees,
  looksLikeMovedRepository,
  planBranchCleanup,
  removeConfirmCopy,
  removeOutcomeMessage,
  sectionVisibility,
  visibleWorktrees,
  worktreeTabLabel,
} from '@/lib/worktreeCopy'

function wt(over: Partial<Worktree> = {}): Worktree {
  return {
    name: 'project-feature',
    folder_name: 'project-feature',
    path: 'C:/code/project-feature',
    branch: 'feature',
    head_sha: 'abc1234',
    is_main: false,
    is_current: false,
    is_locked: false,
    state: 'ok',
    is_run_worktree: false,
    ...over,
  }
}

const dirt = (modified: number, untracked: number): DirtyCount => ({ modified, untracked })

describe('section visibility', () => {
  // Three states, not two: a worktree that exists is never hidden behind a
  // preference, because a checkout the user cannot see is one they cannot open,
  // remove, or repair when it goes wrong.
  it('shows nothing when the setting is off and none exist', () => {
    expect(sectionVisibility(false, 0)).toEqual({
      show: false,
      canAdd: false,
      explainNoAdd: false,
    })
  })

  it('shows what exists when the setting is off, but withholds Add', () => {
    expect(sectionVisibility(false, 1)).toEqual({ show: true, canAdd: false, explainNoAdd: true })
  })

  it('shows everything when the setting is on', () => {
    expect(sectionVisibility(true, 0)).toEqual({ show: true, canAdd: true, explainNoAdd: false })
    expect(sectionVisibility(true, 3)).toEqual({ show: true, canAdd: true, explainNoAdd: false })
  })
})

describe('dirty summary', () => {
  it('says plainly when there is nothing to lose', () => {
    expect(dirtySummary(dirt(0, 0))).toContain('nothing unsaved')
  })

  it('counts modified and untracked separately', () => {
    const text = dirtySummary(dirt(2, 3))
    expect(text).toContain('2 changed files')
    expect(text).toContain('3 new files')
  })

  it('calls out that untracked files have never been saved anywhere', () => {
    // The one case with no way back, so it cannot read the same as a modified
    // tracked file that history can always restore.
    const text = dirtySummary(dirt(0, 1))
    expect(text).toContain('never been saved anywhere')
    expect(text).toContain('gone for good')
  })

  it('uses singular wording for one file', () => {
    expect(dirtySummary(dirt(1, 0))).toContain('1 changed file')
    expect(dirtySummary(dirt(1, 0))).not.toContain('1 changed files')
  })

  it('flags unrecoverable work only when untracked files are present', () => {
    expect(hasUnrecoverableWork(dirt(5, 0))).toBe(false)
    expect(hasUnrecoverableWork(dirt(0, 1))).toBe(true)
  })
})

describe('broken rows', () => {
  // The UI offers one action, never a choice between prune and repair, so this
  // mapping is load-bearing rather than cosmetic.
  it('offers prune for a folder that is gone', () => {
    expect(brokenAction('missing')).toBe('prune')
    expect(brokenExplanation('missing')).toContain('no longer on your computer')
  })

  it('offers repair for a folder that moved', () => {
    expect(brokenAction('moved')).toBe('repair')
    expect(brokenExplanation('moved')).toContain('still there')
  })

  it('offers nothing for a healthy worktree', () => {
    expect(brokenAction('ok')).toBeNull()
    expect(brokenExplanation('ok')).toBeNull()
  })

  it('says whether files are still on disk, so nothing is chosen blind', () => {
    // "Broken does not mean gone" -- the user needs to know what is at risk
    // before picking an action.
    expect(brokenExplanation('missing')).toContain('Nothing is at risk')
    expect(brokenExplanation('moved')).toContain('Your files are still there')
  })
})

describe('a moved main repository', () => {
  it('is recognised when every linked worktree breaks at once', () => {
    const list = [
      wt({ is_main: true, state: 'ok' }),
      wt({ folder_name: 'a', state: 'moved' }),
      wt({ folder_name: 'b', state: 'moved' }),
    ]
    expect(looksLikeMovedRepository(list)).toBe(true)
  })

  it('is not claimed when only one is broken', () => {
    const list = [
      wt({ is_main: true }),
      wt({ folder_name: 'a', state: 'moved' }),
      wt({ folder_name: 'b', state: 'ok' }),
    ]
    expect(looksLikeMovedRepository(list)).toBe(false)
  })

  it('is not claimed for a single broken worktree', () => {
    // One broken row is just one broken row; it needs its own explanation, not
    // a "your repository moved" story that may not be true.
    expect(looksLikeMovedRepository([wt({ is_main: true }), wt({ state: 'missing' })])).toBe(false)
  })
})

describe('remove confirm copy', () => {
  it('promises the branch survives', () => {
    const copy = removeConfirmCopy(wt(), dirt(0, 0))
    expect(copy.title).toContain('project-feature')
    expect(copy.branchNote).toContain('feature branch stays')
  })

  it('states the counts in the body', () => {
    const copy = removeConfirmCopy(wt(), dirt(1, 2))
    expect(copy.body).toContain('1 changed file')
    expect(copy.body).toContain('2 new files')
  })
})

describe('branch cleanup after removal', () => {
  it('offers nothing when the worktree had no branch', () => {
    expect(planBranchCleanup(null, true, null, 'ask')).toEqual({ kind: 'none' })
  })

  it('asks by default, merged or not', () => {
    expect(planBranchCleanup('feature', true, null, 'ask')).toEqual({
      kind: 'ask',
      branch: 'feature',
      remote: null,
      merged: true,
    })
    expect(planBranchCleanup('feature', false, null, 'ask')).toMatchObject({ kind: 'ask' })
  })

  it('stays silent when the user chose to always keep branches', () => {
    expect(planBranchCleanup('feature', true, 'origin', 'keep')).toEqual({ kind: 'none' })
  })

  it('deletes without asking once that choice is remembered', () => {
    expect(planBranchCleanup('feature', true, 'origin', 'delete')).toEqual({
      kind: 'delete',
      branch: 'feature',
      remote: 'origin',
    })
  })

  it('still asks about an unmerged branch even when delete is remembered', () => {
    // The remembered answer was given about a finished branch. Applying it to
    // commits that exist nowhere else would destroy work never offered up.
    expect(planBranchCleanup('feature', false, 'origin', 'delete')).toMatchObject({
      kind: 'ask',
      merged: false,
    })
  })

  it('carries the remote through so the dialog can offer to delete it there', () => {
    expect(planBranchCleanup('feature', true, 'origin', 'ask')).toMatchObject({ remote: 'origin' })
  })
})

describe('removal outcomes', () => {
  it('reports success and reminds the branch is kept', () => {
    const msg = removeOutcomeMessage(
      { kind: 'removed', branch: 'feature', branch_merged: true, branch_remote: null },
      'project-feature'
    )
    expect(msg.tone).toBe('success')
    expect(msg.detail).toContain('feature branch is still here')
  })

  it('explains a locked folder without a raw permission error', () => {
    const msg = removeOutcomeMessage({ kind: 'refusedLocked', path: 'C:/x' }, 'x')
    expect(msg.text).toContain('still using that folder')
    expect(msg.detail).toContain('try again')
    // GitWyrm must never be the culprit, and should say so.
    expect(msg.detail).toContain('already let go')
    expect(JSON.stringify(msg)).not.toMatch(/permission denied|os error/i)
  })

  it('says a partial removal left the folder half there', () => {
    const msg = removeOutcomeMessage({ kind: 'partiallyRemoved', path: 'C:/x' }, 'x')
    expect(msg.text).toContain('partly removed')
    expect(msg.tone).toBe('error')
  })

  it('carries the counts through a dirty refusal', () => {
    const msg = removeOutcomeMessage(
      { kind: 'refusedDirty', modified: 1, untracked: 4 },
      'project-feature'
    )
    expect(msg.detail).toContain('1 changed file')
    expect(msg.detail).toContain('4 new files')
  })
})

describe('branch cleanup offer', () => {
  it('offers deletion only when the work is saved elsewhere', () => {
    expect(branchCleanupOffer('feature', true).canDelete).toBe(true)
  })

  it('keeps an unmerged branch and says why', () => {
    const offer = branchCleanupOffer('feature', false)
    expect(offer.canDelete).toBe(false)
    expect(offer.text).toContain("isn't saved anywhere else")
  })
})

describe('plain language', () => {
  const allCopy = [
    brokenExplanation('missing'),
    brokenExplanation('moved'),
    dirtySummary(dirt(1, 1)),
    branchHeldMessage('feature', 'project-feature'),
    branchCleanupOffer('feature', true).text,
    branchCleanupOffer('feature', false).text,
    removeConfirmCopy(wt(), dirt(1, 1)).body,
    removeOutcomeMessage({ kind: 'refusedLocked', path: 'x' }, 'x').detail,
  ]
    .filter(Boolean)
    .join(' ')

  it('never leaks raw git plumbing at the user', () => {
    // The line this draws is deliberate. "Worktree" is NOT on this list: it is
    // the name of the thing, the word git prints, and the word every AI tool's
    // docs use -- so we teach it rather than inventing a friendlier synonym the
    // user could never search for. What stays out is git's internal plumbing
    // and the flags it expects you to already know.
    for (const plumbing of [
      'HEAD',
      'refspec',
      'reflog',
      'detached',
      '--force',
      'index',
      'fatal',
      'gitdir',
    ]) {
      expect(allCopy).not.toContain(plumbing)
    }
  })

  it('teaches the real word instead of renaming the concept', () => {
    // A user who read about worktrees in an AI tool's docs must recognise this
    // screen, and a user who hits a raw git message must find the same word.
    expect(allCopy).toContain('worktree')
  })

  it('never tells the user to pass a flag', () => {
    expect(allCopy).not.toMatch(/\s--\w/)
  })
})

describe('branch held elsewhere', () => {
  it('names the folder rather than reporting that the branch is in use', () => {
    const msg = branchHeldMessage('feature', 'project-feature')
    expect(msg).toContain('project-feature')
    expect(msg).toContain('one place at a time')
    // Names it as a worktree, so the user can find it in the section.
    expect(msg).toContain('worktree')
  })
})

describe('tab labels', () => {
  it('distinguishes sibling checkouts of one project', () => {
    expect(worktreeTabLabel('project-feature', 'feature')).toBe('project-feature · feature')
    expect(worktreeTabLabel('project', 'main')).toBe('project · main')
    // Two tabs of the same repository must never read identically.
    expect(worktreeTabLabel('project', 'main')).not.toBe(worktreeTabLabel('project-feature', 'feature'))
  })

  it('falls back to the folder name when HEAD is detached', () => {
    expect(worktreeTabLabel('project-feature', null)).toBe('project-feature')
  })
})

describe('linked worktrees', () => {
  it('excludes the main checkout, which is always present', () => {
    const list = [wt({ is_main: true, folder_name: 'project' }), wt()]
    expect(linkedWorktrees(list)).toHaveLength(1)
    expect(linkedWorktrees(list)[0].folder_name).toBe('project-feature')
  })
})

describe('visible worktrees', () => {
  it('draws nothing when only the main checkout exists', () => {
    // A lone main row reads as "you have one worktree", which it is not.
    expect(visibleWorktrees([wt({ is_main: true, folder_name: 'project' })])).toEqual([])
  })

  it('draws the main checkout once there is a sibling to tell it apart from', () => {
    const list = [wt({ is_main: true, folder_name: 'project' }), wt()]
    expect(visibleWorktrees(list)).toEqual(list)
  })

  it('draws nothing for an empty list', () => {
    expect(visibleWorktrees([])).toEqual([])
  })
})
