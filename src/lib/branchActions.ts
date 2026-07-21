import type { BranchInfo } from '@/lib/bindings'

import { plural } from '@/lib/gitDisplay'

const commits = (n: number) => plural(n, 'commit')

/**
 * How a branch's relationship to its remote should read in the UI.
 *
 * `ahead`/`behind` are only meaningful when the branch is tracking something
 * -- see the note on `branchSync` for why the raw counts on `BranchInfo`
 * cannot carry this on their own.
 */
export interface BranchSync {
  ahead: number
  behind: number
  /** Set when the branch's state is not a simple count. */
  marker: 'new' | 'gone' | null
  /** Short text for a badge, or null when there is nothing to report. */
  text: string | null
  /** Longer wording for a tooltip. */
  title: string | null
}

/**
 * The single reading of a branch's sync state for display.
 *
 * `BranchInfo.ahead`/`behind` come from `SyncState::counts()` in Rust, which
 * reports (0, 0) for every state except `diverged`. So a branch with five
 * unpushed commits and a branch that exactly matches its upstream are
 * numerically identical, and anything reading the raw counts shows nothing
 * for both. Read `sync` instead, which keeps them apart.
 */
export function branchSync(branch: BranchInfo): BranchSync {
  const none: BranchSync = { ahead: 0, behind: 0, marker: null, text: null, title: null }

  switch (branch.sync.kind) {
    case 'diverged': {
      const { ahead, behind } = branch.sync
      const text = `${ahead ? `↑${ahead}` : ''}${ahead && behind ? ' ' : ''}${behind ? `↓${behind}` : ''}`
      const parts = [
        ahead ? `${commits(ahead)} to send` : '',
        behind ? `${commits(behind)} to get` : '',
      ].filter(Boolean)
      return { ahead, behind, marker: null, text: text || null, title: parts.join(', ') || null }
    }
    case 'never_pushed':
      return {
        ...none,
        marker: 'new',
        text: 'new',
        title: 'Not sent to the remote yet',
      }
    case 'upstream_gone':
      return {
        ...none,
        marker: 'gone',
        text: 'gone',
        title: 'The branch this tracked is no longer on the remote',
      }
    case 'in_sync':
      return none
  }
}

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
 *
 * `host` names where the commits are going -- "GitHub", "GitLab" -- so the
 * menu says what the user recognizes instead of the generic "the remote".
 * Pass null when the host is self-hosted or unrecognized.
 */
export function branchActions(branch: BranchInfo, host?: string | null): BranchActions {
  const hidden = { show: false, label: '' }
  const to = host ? `to ${host}` : 'to the remote'
  const from = host ? `from ${host}` : 'from the remote'
  const publish = host ? `Publish this branch to ${host}` : 'Publish this branch to the remote'

  switch (branch.sync.kind) {
    case 'diverged': {
      const { ahead, behind } = branch.sync
      return {
        push: ahead > 0
          ? { show: true, label: `Send ${commits(ahead)} ${to}` }
          : hidden,
        // A branch that has moved on both sides needs a real merge, which
        // needs a working tree; that only happens once it is checked out.
        pull: behind > 0 && ahead === 0
          ? { show: true, label: `Get ${commits(behind)} ${from}` }
          : hidden,
        setUpstream: hidden,
      }
    }
    case 'never_pushed':
      return {
        push: { show: true, label: publish },
        pull: hidden,
        setUpstream: hidden,
      }
    case 'upstream_gone':
      return {
        push: { show: true, label: publish },
        pull: hidden,
        setUpstream: { show: true, label: 'Reconnect to the remote branch' },
      }
    case 'in_sync':
      return { push: hidden, pull: hidden, setUpstream: hidden }
  }
}
