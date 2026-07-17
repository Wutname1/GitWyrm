import type { BranchInfo, BranchList } from '@/lib/bindings'

/** dataTransfer MIME type carrying a dragged ref pill's identity. */
export const REF_DND_MIME = 'application/gitwyrm-ref'

export interface DraggedRef {
  name: string
  type: 'head' | 'branch' | 'remote' | 'tag'
}

export type SyncDirection = 'incoming' | 'outgoing'

/** A resolved, valid drag pairing between a local branch and its upstream. */
export interface RefSyncPair {
  /** The local branch and its ahead/behind vs its upstream. */
  branch: BranchInfo
  /** The upstream remote-tracking ref name, e.g. `origin/main`. */
  upstream: string
  /** incoming = remote dropped onto local; outgoing = local dropped onto remote. */
  direction: SyncDirection
}

/**
 * Given a dragged ref (source) dropped onto another ref (target), resolve the
 * local-branch/upstream pairing if - and only if - the two form a tracking
 * pair. A drop is valid when one side is a local branch (head/branch) and the
 * other is exactly that branch's configured upstream. Returns null for any
 * other combination (tags, unrelated branches, a branch with no upstream, etc.).
 */
export function resolveSyncPair(
  source: DraggedRef,
  target: DraggedRef,
  branches: BranchList
): RefSyncPair | null {
  const isLocal = (t: DraggedRef['type']) => t === 'head' || t === 'branch'

  const localSide = isLocal(source.type) ? source : isLocal(target.type) ? target : null
  const remoteSide = source.type === 'remote' ? source : target.type === 'remote' ? target : null
  if (!localSide || !remoteSide || localSide === remoteSide) return null

  const branch = branches.local.find((b) => b.name === localSide.name)
  if (!branch || !branch.upstream || branch.upstream !== remoteSide.name) return null

  // Direction is set by which pill was dragged: the source is what's being
  // "brought to" the target. Remote source -> update local (incoming);
  // local source -> update remote (outgoing).
  const direction: SyncDirection = source.type === 'remote' ? 'incoming' : 'outgoing'
  return { branch, upstream: branch.upstream, direction }
}
