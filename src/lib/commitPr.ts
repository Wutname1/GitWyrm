import type { PrSummary, RefInfo } from '@/lib/bindings'

/**
 * What matching needs from a commit: its sha, its summary, and any refs
 * pointing at it.
 *
 * Deliberately narrower than `CommitEntry` so both callers fit. The commit
 * drawer works from a `CommitDetail`, which carries no refs at all --
 * it passes an empty list and the branch-tip route simply does not fire, which
 * is the honest outcome rather than a wrong guess.
 */
export interface MatchableCommit {
  sha: string
  summary: string
  refs: RefInfo[]
}

/**
 * How a commit was tied to a pull request. Shown to the user, so each value has
 * to be something we can say plainly rather than an internal label.
 */
export type PrMatchReason = 'merge' | 'squash' | 'branch' | 'commit'

export interface CommitPrMatch {
  number: number
  title: string
  /** Browser page for the pull request. */
  webUrl: string
  draft: boolean
  reason: PrMatchReason
}

/**
 * `Merge pull request #218 from owner/branch` -- what GitHub writes when a pull
 * request is merged with a merge commit.
 *
 * Anchored to the start so a commit that merely mentions the phrase in its body
 * does not match; the summary is the only line this is read from.
 */
const MERGE_SUMMARY = /^Merge pull request #(\d+)\b/

/**
 * The trailing `(#218)` GitHub appends when a pull request is squashed or
 * rebased. Anchored to the end of the summary because a number in the middle of
 * a sentence is far more likely to be an issue reference than this commit's own
 * pull request.
 */
const SQUASH_SUMMARY = /\(#(\d+)\)\s*$/

/**
 * The pull request number a commit message names, or null.
 *
 * Only the summary is read. Bodies routinely cite other pull requests and
 * issues ("follows #204"), and treating those as this commit's own pull request
 * would light up the wrong item.
 */
export function prNumberFromMessage(summary: string): { number: number; reason: PrMatchReason } | null {
  const merge = MERGE_SUMMARY.exec(summary)
  if (merge) return { number: Number(merge[1]), reason: 'merge' }
  const squash = SQUASH_SUMMARY.exec(summary)
  if (squash) return { number: Number(squash[1]), reason: 'squash' }
  return null
}

/**
 * The branch names a commit is the tip of, ignoring the pseudo-refs.
 *
 * Remote-tracking refs are included with their remote prefix stripped, since a
 * pull request names its source branch plainly (`head_ref` is `my-feature`, not
 * `origin/my-feature`).
 */
function tipBranchNames(commit: MatchableCommit): string[] {
  const out: string[] = []
  for (const ref of commit.refs) {
    if (ref.type === 'branch') out.push(ref.name)
    else if (ref.type === 'remote') {
      const slash = ref.name.indexOf('/')
      out.push(slash === -1 ? ref.name : ref.name.slice(slash + 1))
    }
  }
  return out
}

/**
 * The pull request a commit belongs to, or null.
 *
 * Three routes, tried in the order of how sure each one is:
 *
 * 1. The commit message names it (`Merge pull request #218`, `... (#218)`).
 *    This is the only route that works for pull requests that are already
 *    merged and closed, since those are no longer in the open list.
 * 2. The commit's sha appears in a pull request's own commit list. Broadest
 *    coverage for open work -- every commit on the branch matches, not just
 *    the tip -- but the caller has to have fetched those lists.
 * 3. The commit is the tip of a branch that an open pull request is built from.
 *    Costs nothing extra and covers the common case before the lists arrive.
 *
 * A message match wins over the sha lists because it is specific to this commit;
 * the sha route is checked before the branch route because it identifies the
 * commit itself rather than the branch that happens to point at it.
 *
 * `shaToPr` maps a full commit sha to the pull request number whose commit list
 * contains it. Callers that have not fetched any lists pass an empty map, and
 * matching quietly falls back to the other two routes.
 */
export function matchCommitToPr(
  commit: MatchableCommit,
  prs: PrSummary[],
  shaToPr: ReadonlyMap<string, number> = new Map()
): CommitPrMatch | null {
  const byNumber = (number: number, reason: PrMatchReason): CommitPrMatch | null => {
    const pr = prs.find((p) => p.number === number)
    if (!pr) return null
    return {
      number: pr.number,
      title: pr.title,
      webUrl: pr.html_url,
      draft: pr.draft,
      reason,
    }
  }

  const fromMessage = prNumberFromMessage(commit.summary)
  if (fromMessage) {
    const hit = byNumber(fromMessage.number, fromMessage.reason)
    // A merged pull request has dropped out of the open list, so there is
    // nothing to link to -- but the number in the message is still the truth
    // about this commit, so report it with what little we know.
    if (hit) return hit
    return {
      number: fromMessage.number,
      title: `Pull request #${fromMessage.number}`,
      webUrl: '',
      draft: false,
      reason: fromMessage.reason,
    }
  }

  const fromSha = shaToPr.get(commit.sha)
  if (fromSha != null) {
    const hit = byNumber(fromSha, 'commit')
    if (hit) return hit
  }

  const branches = tipBranchNames(commit)
  if (branches.length > 0) {
    const pr = prs.find((p) => branches.includes(p.head_ref))
    if (pr) {
      return {
        number: pr.number,
        title: pr.title,
        webUrl: pr.html_url,
        draft: pr.draft,
        reason: 'branch',
      }
    }
  }

  return null
}

/** Plain-language explanation of why this commit is tied to this pull request. */
export function matchExplanation(match: CommitPrMatch): string {
  switch (match.reason) {
    case 'merge':
      return `This commit merged pull request #${match.number}.`
    case 'squash':
      return `This commit came from pull request #${match.number}.`
    case 'commit':
      return `This commit is part of pull request #${match.number}.`
    case 'branch':
      return `This commit is the latest on the branch for pull request #${match.number}.`
  }
}
