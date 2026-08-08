import type { PreviewCommit } from '@/lib/bindings'

/**
 * Which outcome the preview should draw. Mirrors the sync modal's buttons.
 *
 * `replace` and `reset` are the two destructive ones and differ only in which
 * side loses work: `replace` (force-push) overwrites THEIR commits, `reset`
 * throws away OURS. Both get the same struck-through treatment.
 */
export type PreviewMode = 'blend' | 'stack' | 'replace' | 'catch-up' | 'reset'

/** What the previewed action does to a given commit. */
export type Fate = 'keep' | 'discard' | 'replay' | 'merge'

export interface Row {
  key: string
  sha: string | null
  summary: string
  side: 'ours' | 'theirs' | 'merge'
  fate: Fate
}

/**
 * Turn the two arms of a divergence into the rows the ladder draws.
 *
 * This is the function that decides which commits appear struck through, so it
 * is what stands between the user and an unexpected loss of work: if `replace`
 * ever stopped marking their arm as discarded, the force-push button would look
 * safe. Newest-first ordering is inherited from the backend walk.
 */
export function buildRows(
  ours: PreviewCommit[],
  theirs: PreviewCommit[],
  mode: PreviewMode | null,
  ourName: string,
  theirName: string
): Row[] {
  const oursRows: Row[] = ours.map((c) => ({
    key: `o-${c.sha}`,
    sha: c.sha,
    summary: c.summary,
    side: 'ours',
    fate: 'keep',
  }))
  const theirsRows: Row[] = theirs.map((c) => ({
    key: `t-${c.sha}`,
    sha: c.sha,
    summary: c.summary,
    side: 'theirs',
    fate: 'keep',
  }))

  switch (mode) {
    case 'replace':
      // Force-push: the cloud's own commits are overwritten and gone.
      return [...oursRows, ...theirsRows.map((r) => ({ ...r, fate: 'discard' as const }))]
    case 'reset':
      // Hard reset: we abandon our own commits and take their line as-is. The
      // mirror image of `replace` -- our arm is the one that is lost.
      return [...oursRows.map((r) => ({ ...r, fate: 'discard' as const })), ...theirsRows]
    case 'stack':
      // Rebase: our commits are re-created on top of theirs -- same work, new
      // commit IDs, which is why they read as "rebuilt" rather than kept.
      return [...oursRows.map((r) => ({ ...r, fate: 'replay' as const })), ...theirsRows]
    case 'blend':
      // Merge: both arms survive and a new commit joins them.
      return [
        {
          key: 'merge',
          sha: null,
          summary: `Blend ${theirName} into ${ourName}`,
          side: 'merge',
          fate: 'merge',
        },
        ...oursRows,
        ...theirsRows,
      ]
    case 'catch-up':
      // Fast-forward: nothing of ours is in the way, so theirs simply lands.
      return theirsRows
    default:
      return [...oursRows, ...theirsRows]
  }
}

/** How many commits the previewed action would throw away. */
export function discardedCount(rows: Row[]): number {
  return rows.filter((r) => r.fate === 'discard').length
}
