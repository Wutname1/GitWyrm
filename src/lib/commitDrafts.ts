/**
 * Unfinished commit messages, keyed by repo id.
 *
 * These live apart from the commit box's own state because the box is one
 * component reused across repo tabs. Holding the message in the component means
 * it belongs to the box rather than to a repo, so switching tabs carries a
 * half-written message -- or an AI-generated one still in flight -- into
 * whichever repo is on screen. Keying by repo is what makes a message reach
 * only the repo it was written for.
 */

/** An unfinished commit message being written for one repo. */
export interface CommitDraft {
  summary: string
  description: string
  /** Whether the "Amend previous commit" box was ticked. */
  amend: boolean
}

export type CommitDraftMap = Record<string, CommitDraft>

export const EMPTY_DRAFT: CommitDraft = {
  summary: '',
  description: '',
  amend: false,
}

/**
 * A draft with nothing in it. Amend counts as content: ticking the box before
 * the previous commit has loaded leaves both text fields blank, and dropping
 * that draft would silently untick it.
 */
export function draftIsEmpty(d: CommitDraft): boolean {
  return !d.summary.trim() && !d.description.trim() && !d.amend
}

/**
 * Applies a partial edit to one repo's draft, merging with whatever is already
 * there so typing in one field never clears another.
 *
 * Returns the same map when nothing changed, so store subscribers do not
 * re-render on a no-op. Drafts that end up empty are dropped rather than left
 * as a blank entry per repo the user has merely clicked into.
 */
export function applyDraft(
  drafts: CommitDraftMap,
  repoId: string,
  patch: Partial<CommitDraft>,
): CommitDraftMap {
  const next = { ...(drafts[repoId] ?? EMPTY_DRAFT), ...patch }
  if (draftIsEmpty(next)) return clearDraft(drafts, repoId)
  return { ...drafts, [repoId]: next }
}

/** Forgets a repo's draft, e.g. once its commit is written or its tab closes. */
export function clearDraft(
  drafts: CommitDraftMap,
  repoId: string,
): CommitDraftMap {
  if (!drafts[repoId]) return drafts
  const rest = { ...drafts }
  delete rest[repoId]
  return rest
}
