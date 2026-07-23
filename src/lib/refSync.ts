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

/**
 * Any valid drop pairing. A local branch and its own upstream form a `tracking`
 * pair (push / pull / rebase against the cloud copy). Any other ref dropped
 * onto a LOCAL branch is a `branches` pair.
 *
 * Branch-pair direction is set by which ref CAN move. A remote-tracking ref
 * can't be moved locally, so it is always the `source` (where commits come
 * from) and the local branch is the `target` (the one that catches up). When
 * both refs are local, the physical drag decides: dragging a branch onto
 * another reads as "put this branch here", so the DRAGGED branch is the target
 * that moves and the branch it landed on is the source.
 */
export type DropPair =
  | ({ kind: 'tracking' } & RefSyncPair)
  | { kind: 'branches'; source: DraggedRef; target: DraggedRef }

const isLocalBranch = (ref: DraggedRef, branches: BranchList) =>
  (ref.type === 'head' || ref.type === 'branch') &&
  branches.local.some((b) => b.name === ref.name)

/**
 * `dragged` is the ref the user picked up; `droppedOn` is the ref it was
 * released over. For tracking pairs the two are handed to `resolveSyncPair`
 * as-is (its push/pull direction keys off which one was dragged). For branch
 * pairs the mover becomes the operation's `target` -- see DropPair.
 */
export function resolveDropPair(
  dragged: DraggedRef,
  droppedOn: DraggedRef,
  branches: BranchList
): DropPair | null {
  if (dragged.type === 'tag' || droppedOn.type === 'tag') return null
  if (dragged.name === droppedOn.name) return null

  const tracking = resolveSyncPair(dragged, droppedOn, branches)
  if (tracking) return { kind: 'tracking', ...tracking }

  const draggedLocal = isLocalBranch(dragged, branches)
  const droppedLocal = isLocalBranch(droppedOn, branches)

  // The target is whichever ref can actually move. A remote ref can't, so the
  // local side receives its commits. With both local, the dragged one moves.
  if (draggedLocal && !droppedLocal) {
    // Dragged a local branch onto a remote/other ref: the dragged branch moves.
    return { kind: 'branches', source: droppedOn, target: dragged }
  }
  if (droppedLocal && !draggedLocal) {
    // Dragged a remote ref onto a local branch: the local branch receives it.
    return { kind: 'branches', source: dragged, target: droppedOn }
  }
  if (draggedLocal && droppedLocal) {
    // Both local: the dragged branch is the one being placed.
    return { kind: 'branches', source: droppedOn, target: dragged }
  }

  // Neither side is a local branch (e.g. remote onto remote): nothing to do.
  return null
}
