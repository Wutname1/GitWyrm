import { useEffect, useRef } from 'react'
import { useUiStore } from '@/stores/uiStore'

/** The WIP row's contents, kept around so it can be held on screen. */
export interface PendingSnapshot {
  stagedCount: number
  unstagedCount: number
  filesChanged: number
  additions: number
  deletions: number
}

/**
 * Longest we hold a stale WIP row before giving up. The commit is already made
 * by this point, so the only cost of the timeout firing is the old two-step
 * reflow -- far better than leaving a phantom row wedged at the top of the
 * graph if the commit never shows up (a filtered log, a failed refetch).
 */
const HOLD_TIMEOUT_MS = 10_000

/**
 * Keeps the WIP row on screen across a commit.
 *
 * Committing clears the working tree and adds a commit, but those reach the UI
 * as two separate queries: `status` comes back almost immediately (tree is
 * clean, so the WIP row would vanish) while `log` has to walk history and lands
 * a beat later. Left alone the graph collapses by a row, then pushes everything
 * back down when the new commit appears -- two visible jolts for one action.
 *
 * So from the moment a commit succeeds until its sha is actually in the graph,
 * we go on drawing the last snapshot of the row. The row is then replaced by
 * the new commit in a single step, with nothing shifting underneath it.
 *
 * Only a real commit starts a hold (the store field is set by the commit
 * mutation), so discarding or staging away every change still drops the row
 * immediately -- there is no commit coming and nothing to wait for.
 */
export function usePendingRowHold(
  hasChanges: boolean,
  snapshot: PendingSnapshot,
  isLoaded: (sha: string) => boolean,
): { show: boolean; snapshot: PendingSnapshot } {
  const awaiting = useUiStore((s) => s.awaitingCommitSha)
  const commitLanding = useUiStore((s) => s.commitLanding)

  // Last contents the row had while there really were changes, so the held row
  // shows what was committed rather than an empty one.
  const lastSnapshot = useRef<PendingSnapshot>(snapshot)
  if (hasChanges) lastSnapshot.current = snapshot

  const landed = awaiting != null && isLoaded(awaiting)

  // The commit is in the graph: release, letting both rows swap in one paint.
  useEffect(() => {
    if (landed) commitLanding(null)
  }, [landed, commitLanding])

  // Backstop so a commit that never appears cannot strand the row forever.
  useEffect(() => {
    if (awaiting == null) return
    const timer = window.setTimeout(() => commitLanding(null), HOLD_TIMEOUT_MS)
    return () => window.clearTimeout(timer)
  }, [awaiting, commitLanding])

  const holding = awaiting != null && !landed
  return {
    show: hasChanges || holding,
    snapshot: hasChanges ? snapshot : lastSnapshot.current,
  }
}
