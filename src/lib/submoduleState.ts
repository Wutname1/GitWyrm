import type { SubmoduleStatus } from '@/lib/bindings'

/**
 * What a linked project's state means, in the terms a person decides with.
 *
 * The raw fields say what git sees; this says what to do about it. The reported
 * confusion was a folder left behind by someone else's update, which the app
 * described as "changed" and offered to fix under "Undo my changes" -- blaming
 * the user for a state they had not caused, and hiding the one action that
 * would fix it.
 */
export type SubmoduleSituation =
  /** Recorded but never downloaded, so the folder is empty. */
  | 'not-downloaded'
  /** Sitting exactly where this project expects. */
  | 'matches'
  /** Older than this project expects -- usually a pull that left it behind. */
  | 'behind-project'
  /** Newer than this project expects -- an update to save, or someone's edit. */
  | 'ahead-of-project'
  /** Off on a different line of work entirely. */
  | 'diverged'

export interface SubmoduleReading {
  situation: SubmoduleSituation
  /** Short label for the row's right edge. */
  label: string
  /** Tailwind text colour class for that label. */
  tone: string
  /** One sentence saying what this means, not what git calls it. */
  meaning: string
  /** True when matching the project again is worth offering. */
  canMatchProject: boolean
  /**
   * True when the fix is routine rather than destructive.
   *
   * Catching up a folder someone else moved discards nothing of the user's, so
   * it must not be styled or worded as though it might.
   */
  matchIsSafe: boolean
}

/** Read a submodule's raw status as a situation and what to do about it. */
export function readSubmodule(s: SubmoduleStatus): SubmoduleReading {
  if (s.state === 'uninitialized') {
    return {
      situation: 'not-downloaded',
      label: 'not downloaded',
      tone: 'text-muted-foreground',
      meaning: 'This folder is empty. Download it to get the files.',
      canMatchProject: true,
      matchIsSafe: true,
    }
  }

  if (s.state === 'in_sync') {
    return {
      situation: 'matches',
      label: '',
      tone: 'text-muted-foreground',
      meaning: 'Up to date with the version this project expects.',
      canMatchProject: false,
      matchIsSafe: true,
    }
  }

  // Both directions: a real divergence, so matching the project throws away
  // whatever this folder has of its own.
  if (s.ahead > 0 && s.behind > 0) {
    return {
      situation: 'diverged',
      label: 'different version',
      tone: 'text-[var(--gw-amber)]',
      meaning:
        'This folder is on a different line of work than the project expects. Matching the project would drop what is only here.',
      canMatchProject: true,
      matchIsSafe: false,
    }
  }

  if (s.ahead > 0) {
    return {
      situation: 'ahead-of-project',
      label: `${s.ahead} newer`,
      tone: 'text-[var(--gw-amber)]',
      meaning:
        'This folder is newer than the project expects. Commit to save the update, or match the project to drop it.',
      canMatchProject: true,
      matchIsSafe: false,
    }
  }

  // Behind only. Nothing of the user's is here to lose -- the folder simply has
  // not caught up yet, which is what a pull leaves when it cannot move it.
  return {
    situation: 'behind-project',
    label: `${s.behind} behind`,
    tone: 'text-[var(--gw-amber)]',
    meaning:
      'This folder has not caught up with the version this project expects. Nothing of yours is here to lose.',
    canMatchProject: true,
    matchIsSafe: true,
  }
}

/**
 * Whether committing inside this folder would strand the work.
 *
 * A detached HEAD is the ordinary resting state after an update, so this is not
 * a fault -- but a commit made there belongs to no branch and is easy to lose,
 * which is worth saying before it happens rather than after.
 */
export function isDetached(s: SubmoduleStatus): boolean {
  return s.state !== 'uninitialized' && s.head_branch == null
}

/** Where the nested checkout sits, for the row's second line. */
export function describeHead(s: SubmoduleStatus): string {
  if (s.state === 'uninitialized') return 'Not downloaded'
  if (s.head_branch) return `On ${s.head_branch}`
  return 'Not on a branch'
}
