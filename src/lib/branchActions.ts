import type { BranchInfo } from '@/lib/bindings'

const commits = (n: number) => `${n} commit${n === 1 ? '' : 's'}`

export interface BranchActions {
  /** Send local commits, or publish a branch the remote has never seen. */
  push: { show: boolean; label: string }
  /** Fetch commits from the remote. Only offered when it can fast-forward. */
  pull: { show: boolean; label: string }
  /** Repair a branch whose remote branch is missing. */
  setUpstream: { show: boolean; label: string }
}

/**
 * Which remote actions a branch offers, and how to word them.
 *
 * Reads `sync` rather than the raw ahead/behind counts: those cannot tell
 * "never pushed" apart from "matches its upstream", since both report zero.
 */
export function branchActions(branch: BranchInfo): BranchActions {
  const hidden = { show: false, label: '' }

  switch (branch.sync.kind) {
    case 'diverged': {
      const { ahead, behind } = branch.sync
      return {
        push: ahead > 0
          ? { show: true, label: `Send ${commits(ahead)} to the remote` }
          : hidden,
        // A branch that has moved on both sides needs a real merge, which
        // needs a working tree; that only happens once it is checked out.
        pull: behind > 0 && ahead === 0
          ? { show: true, label: `Get ${commits(behind)} from the remote` }
          : hidden,
        setUpstream: hidden,
      }
    }
    case 'never_pushed':
      return {
        push: { show: true, label: 'Publish this branch to the remote' },
        pull: hidden,
        setUpstream: hidden,
      }
    case 'upstream_gone':
      return {
        push: { show: true, label: 'Publish this branch to the remote' },
        pull: hidden,
        setUpstream: { show: true, label: 'Reconnect to the remote branch' },
      }
    case 'in_sync':
      return { push: hidden, pull: hidden, setUpstream: hidden }
  }
}
