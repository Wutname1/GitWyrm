import type { MergeState } from '@/lib/bindings'

/**
 * Pulls the plain branch/ref name out of the operation's stored message.
 *
 * `incoming_label` is the first line of MERGE_MSG (or the stopped commit's
 * subject during a rebase), so for a merge it arrives wrapped in git's own
 * phrasing. Anything that doesn't match is already a bare name.
 */
export function incomingName(label: string | null | undefined): string {
  if (!label) return ''
  const match = label.match(/Merge (?:branch|remote-tracking branch) '([^']+)'/)
  return match ? match[1] : label
}

export interface SideNames {
  /** Name for stage 2 ("ours"). */
  ours: string
  /** Name for stage 3 ("theirs"). */
  theirs: string
  /** True when real branch names were found, so callers can drop the fallback wording. */
  named: boolean
}

/**
 * Branch names for the two sides of a conflict.
 *
 * A rebase inverts what the words mean. Git replays your commits ON TOP OF the
 * other branch, so mid-rebase "ours" is the branch being rebased onto and
 * "theirs" is your own work -- the exact opposite of a merge. Labelling the
 * sides with raw ours/theirs is what makes conflicts so easy to resolve
 * backwards, so the names are resolved per operation rather than assumed.
 */
export function sideNames(
  state: MergeState | null | undefined,
  currentBranch: string | null | undefined
): SideNames {
  const current = currentBranch?.trim() || ''
  const incoming = incomingName(state?.incoming_label)

  if (state?.operation === 'Rebase') {
    // Verified against a real rebase: replaying `feature` onto `main` leaves
    // stage 2 holding main's text and stage 3 holding feature's. So "ours" is
    // the branch being replayed ONTO, and "theirs" is the work being replayed
    // -- which is the branch `incoming` names during a rebase. HEAD is detached
    // mid-rebase, so `current` cannot be trusted for either side here.
    return {
      ours: 'the branch you are replaying onto',
      theirs: incoming || 'your commit being replayed',
      named: false,
    }
  }

  return {
    ours: current || 'your version',
    theirs: incoming || 'incoming version',
    named: !!current && !!incoming,
  }
}
