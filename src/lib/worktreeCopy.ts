/**
 * Plain-language wording for worktree states, refusals, and confirms.
 *
 * Kept out of the components so it can be unit-tested: vitest runs in a node
 * environment with no DOM, so anything worth asserting on has to live in a
 * module like this rather than inside a `.tsx` component.
 *
 * Every string here is written for someone who has never read git's error
 * output. That is the whole reason worktrees are hard: git reports symptoms
 * ("is already used by worktree", "not a working tree", "permission denied")
 * and never the cause or the way out.
 *
 * Plain language means explaining clearly, NOT renaming the concept. "Worktree"
 * is the word git prints, the word every AI tool's documentation uses, and the
 * word the user will type when they search for help -- so it is the word we
 * use, and we teach it. Calling it something friendlier would leave them unable
 * to connect this screen to anything else they read, and would break down the
 * moment a raw git message reached them. What we do simplify is everything
 * around it: "2 files you haven't saved yet", not "unstaged changes".
 *
 * The one place "folder" is still correct is the thing on disk -- a worktree
 * *has* a folder, and that is the concrete noun for where its files live.
 */

import type { DirtyCount, RemoveOutcome, Worktree, WorktreeState } from '@/lib/bindings'

/** "1 file" / "3 files" -- shared shape with the rest of the app. */
function count(n: number, noun: string): string {
  return `${n} ${noun}${n === 1 ? '' : 's'}`
}

/**
 * What a broken row says happened, and whether the files are still on disk.
 *
 * "Broken does not mean gone" is a requirement in its own right: the user needs
 * to know whether anything is at risk before choosing an action, because prune
 * throws away a reference while repair keeps a checkout.
 */
export function brokenExplanation(state: WorktreeState): string | null {
  if (state === 'missing') {
    return "This worktree's folder is no longer on your computer. Nothing is at risk — tidying up just removes the leftover reference."
  }
  if (state === 'moved') {
    return "This worktree's folder moved, so the link to it broke. Your files are still there; repairing points the project back at them."
  }
  return null
}

/** The action a broken row offers. One action, never a choice between two. */
export function brokenAction(state: WorktreeState): 'prune' | 'repair' | null {
  if (state === 'missing') return 'prune'
  if (state === 'moved') return 'repair'
  return null
}

/**
 * The line describing what is unsaved in a worktree about to be removed.
 *
 * Modified and untracked stay separate all the way to the user. "3 changes"
 * would hide the only case with no way back: an untracked file has never been
 * written to history, so discarding it is unrecoverable, while a modified
 * tracked file can always be got back.
 */
export function dirtySummary(dirt: DirtyCount): string {
  const { modified, untracked } = dirt
  if (modified === 0 && untracked === 0) {
    return 'There is nothing unsaved in it.'
  }
  if (modified > 0 && untracked > 0) {
    return `It has ${count(modified, 'changed file')} and ${count(untracked, 'new file')} that have never been saved anywhere.`
  }
  if (untracked > 0) {
    return `It has ${count(untracked, 'new file')} that ${untracked === 1 ? 'has' : 'have'} never been saved anywhere. If you throw ${untracked === 1 ? 'it' : 'them'} away, ${untracked === 1 ? 'it is' : 'they are'} gone for good.`
  }
  return `It has ${count(modified, 'changed file')} you haven't saved yet.`
}

/**
 * Whether the remove confirm should warn extra loudly. Untracked files are the
 * one case with no recovery, so they get their own emphasis rather than being
 * folded into a single "unsaved work" warning.
 */
export function hasUnrecoverableWork(dirt: DirtyCount): boolean {
  return dirt.untracked > 0
}

/** What the removal actually did, phrased as a result the user can act on. */
export function removeOutcomeMessage(
  outcome: RemoveOutcome,
  folderName: string
): { tone: 'success' | 'warning' | 'error'; text: string; detail?: string } {
  switch (outcome.kind) {
    case 'removed':
      return {
        tone: 'success',
        text: `Removed the ${folderName} worktree`,
        detail: outcome.branch
          ? `The ${outcome.branch} branch is still here.`
          : undefined,
      }
    case 'refusedDirty':
      return {
        tone: 'warning',
        text: `The ${folderName} worktree has work in it that isn't saved`,
        detail: dirtySummary({ modified: outcome.modified, untracked: outcome.untracked }),
      }
    case 'refusedLocked':
      return {
        tone: 'error',
        text: 'A program is still using that folder',
        detail:
          'Close any editor, terminal, or program open in it, then try again. GitWyrm has already let go of it.',
      }
    case 'partiallyRemoved':
      return {
        tone: 'error',
        text: 'That folder was only partly removed',
        detail:
          'Some files were deleted before a program stopped the rest. Close anything using the folder and try again to finish.',
      }
  }
}

/**
 * The title and body for the remove confirm.
 *
 * States the counts, calls out that untracked files have never been saved
 * anywhere, and says the branch survives -- the three things the user needs
 * before deciding, and the three git never mentions.
 */
export function removeConfirmCopy(
  worktree: Worktree,
  dirt: DirtyCount
): { title: string; body: string; branchNote: string } {
  return {
    title: `Remove the ${worktree.folder_name} worktree?`,
    body: dirtySummary(dirt),
    branchNote: worktree.branch
      ? `The ${worktree.branch} branch stays exactly as it is — only this checkout of the files goes away.`
      : 'Only this checkout of the files goes away.',
  }
}

/**
 * What to tell the user when a branch is checked out somewhere else.
 *
 * Names the folder, because "already checked out" without a location is the
 * report that sends people hunting through a filesystem.
 */
export function branchHeldMessage(branch: string, folderName: string): string {
  return `${branch} is already checked out in the ${folderName} worktree. A branch can only be checked out in one place at a time.`
}

/** The follow-on offer after a removal, when deleting the branch is safe. */
export function branchCleanupOffer(
  branch: string,
  merged: boolean
): { canDelete: boolean; text: string } {
  if (merged) {
    return {
      canDelete: true,
      text: `The ${branch} branch is finished with — its work is already saved elsewhere. Delete it too?`,
    }
  }
  return {
    canDelete: false,
    text: `The ${branch} branch still has work that isn't saved anywhere else, so it's being kept.`,
  }
}

/**
 * Whether the Worktrees section should be shown at all, and whether Add is
 * offered.
 *
 * Three states, not two. A worktree that *exists* is part of the repository's
 * real state and is never hidden behind a preference -- hiding it would mean
 * the user cannot open, remove, or repair a checkout another tool put there.
 * The setting governs creation; existence governs the section.
 */
export function sectionVisibility(
  settingOn: boolean,
  linkedCount: number
): { show: boolean; canAdd: boolean; explainNoAdd: boolean } {
  if (settingOn) return { show: true, canAdd: true, explainNoAdd: false }
  if (linkedCount > 0) return { show: true, canAdd: false, explainNoAdd: true }
  return { show: false, canAdd: false, explainNoAdd: false }
}

/** Linked worktrees only -- the main checkout is always present. */
export function linkedWorktrees(list: Worktree[]): Worktree[] {
  return list.filter((w) => !w.is_main)
}

/**
 * True when every linked worktree is broken at once, which is what a moved
 * *main repository* looks like. Offering one repair beats showing a list of
 * separately broken rows that all have the same cause.
 */
export function looksLikeMovedRepository(list: Worktree[]): boolean {
  const linked = linkedWorktrees(list)
  if (linked.length < 2) return false
  return linked.every((w) => w.state !== 'ok')
}

/**
 * Tab subtitle that tells sibling checkouts of one repository apart.
 *
 * Two tabs of the same project with identical titles is exactly the ambiguity
 * worktrees introduce, so the branch is always part of the label.
 */
export function worktreeTabLabel(folderName: string, branch: string | null): string {
  return branch ? `${folderName} · ${branch}` : folderName
}
