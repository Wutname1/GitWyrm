import { create } from 'zustand'
import type { DiffSource } from '@/lib/bindings'
import type { SectionKey } from '@/lib/types'

export type CenterView =
  | 'graph'
  | 'diff'
  | 'settings'
  | 'conflict'
  | 'github'
  | 'fileHistory'
  | 'blame'
  | 'repoPicker'

export type ModalKind =
  | 'onboarding'
  | 'tutorial'
  | 'merge'
  | 'remote-sync'
  | 'push-choice'
  | 'newBranch'
  | 'newTag'
  | 'remotes'
  | 'githubConnect'
  | 'addSubmodule'
  | null

export interface GithubItemRef {
  kind: 'pr' | 'issue'
  number: number
}

export type SettingsSection = 'general' | 'behavior' | 'repository' | 'repositoryTags' | 'tags' | 'profiles' | 'ai' | 'security' | 'appearance' | 'logs' | 'about'

/** A local-only tag offered after a push. */
export interface PendingTag {
  name: string
  /**
   * True when the tagged commit isn't on the remote yet, so sending the tag
   * also sends the commits behind it. Worth saying out loud in the prompt.
   */
  carriesCommits: boolean
}

export interface DiffRequest {
  path: string
  source: DiffSource
}

/** File the history or blame view is showing. `sha` pins blame to a commit. */
export interface FileTarget {
  path: string
  sha: string | null
}

interface UiState {
  centerView: CenterView
  selectedSha: string | null
  /**
   * Every selected commit sha in graph (newest-first) order. Holds one entry
   * for a plain click and grows via Ctrl-click (toggle one) or Shift-click
   * (range from the last plain-clicked commit). `selectedSha` is always the
   * anchor the next Shift-click ranges from, and stays inside this list.
   */
  selectedShas: string[]
  diffRequest: DiffRequest | null
  conflictPath: string | null
  /** File shown by the history / blame views. */
  fileTarget: FileTarget | null
  sectionOpen: Record<SectionKey, boolean>
  activeModal: ModalKind
  mergeSource: string | null
  syncSource: string | null
  syncTarget: string | null
  tagTargetSha: string | null
  branchTargetSha: string | null
  /**
   * Local-only tags a finished push left behind, prompting to send them too.
   * Null when there is nothing to ask about.
   */
  tagsToPush: PendingTag[] | null
  /**
   * Remote the manage-remotes modal should open straight into editing, set from
   * the sidebar's Edit so the name/URL boxes are one click away. Null opens the
   * plain list.
   */
  remoteToEdit: string | null
  /** Remote the manage-remotes modal should open straight into deleting. */
  remoteToDelete: string | null
  /** Branch pending a rename / delete confirm, set from any branch menu. */
  branchToRename: string | null
  branchToDelete: string | null
  /**
   * Remote branch pending a delete confirm, split into the remote and the
   * branch as it exists there (`origin` + `feature/x`). Deleting on the server
   * affects everyone, so this always goes through a confirm rather than
   * running straight from the menu.
   */
  remoteBranchToDelete: { remote: string; branch: string } | null
  /**
   * Branch the current branch will be hard-reset to, pending confirm. Set from
   * any branch menu or a branch-onto-branch drop; the target names where the
   * checked-out branch will be rewound to.
   */
  branchToResetTo: string | null
  settingsSection: SettingsSection
  /**
   * Setting the settings view should scroll to and flash, set when a search
   * result is picked. The bumped nonce re-triggers the flash on a repeat pick.
   */
  revealSetting: { id: string; nonce: number } | null
  changesFocusNonce: number
  /** Ref (branch/tag) the graph should scroll to and highlight; bumped nonce re-triggers. */
  revealRef: { name: string; nonce: number } | null
  /** Commit or stash sha the graph should scroll to and select; bumped nonce re-triggers. */
  revealSha: { sha: string; nonce: number } | null
  /**
   * Text typed in the toolbar search box. The graph dims commits that don't
   * match and scrolls to the first that does. Empty string means "not searching".
   */
  commitSearch: string
  /**
   * Signed nonce the search box bumps to step through matches: +1 for next, -1
   * for previous. The graph reads the sign to pick a direction and the changing
   * value to re-trigger. Reset to 0 on a new query.
   */
  searchJumpNonce: number
  /** Bumped when Ctrl+F is pressed, so the toolbar can focus its search box. */
  searchFocusNonce: number
  /**
   * How many loaded commits match the current search. The graph computes it and
   * publishes it so the search box can show "N found" without re-scanning rows.
   * Null when not searching.
   */
  searchMatchCount: number | null
  /**
   * 1-based position of the currently selected match among all matches (so the
   * box can show "3/12"), or 0 when the selection isn't on a match yet. Null
   * when not searching. Published by the graph alongside the count.
   */
  searchMatchIndex: number | null
  /** PR or issue shown in the center view and the actions panel. */
  githubItem: GithubItemRef | null
  /**
   * Bumped when the user tries to drag the "Add a repository" placeholder tab.
   * The picker view watches it and does a little "nuh uh, over here" wiggle
   * instead of letting the tab move.
   */
  repoPickerWiggleNonce: number
  /**
   * True while the "Add a repository" tab exists. It stays open when the user
   * clicks away to a repo, so they can come back to a half-finished clone URL
   * instead of starting over. Closing the tab is what clears it.
   */
  repoPickerOpen: boolean
  /**
   * Lane track the graph drew each stash in, keyed by stash sha. The graph owns
   * this -- the track falls out of lane packing over the loaded rows, so it can
   * shift as more history pages in and cannot be derived from a stash alone.
   * The sidebar reads it to tint its stash icon the same color as the node in
   * the graph, and falls back to a neutral marker for stashes not drawn yet.
   */
  stashTracks: Record<string, number>
  /**
   * Sha of a commit that was just created and has not appeared in the graph
   * yet. Committing clears the working tree before the commit log has been
   * re-read, so the graph would drop its "Uncommitted changes" row and only
   * push the new commit in a beat later -- two jolts for one action. The graph
   * keeps drawing the row until this sha shows up, then swaps them in one step.
   */
  awaitingCommitSha: string | null

  selectCommit: (sha: string | null) => void
  /** Announce a fresh commit the graph should hold its WIP row for. */
  commitLanding: (sha: string | null) => void
  /**
   * Replace the multi-selection wholesale (graph order, newest first). The
   * graph computes ranges/toggles since it owns the row order; `anchor` is the
   * commit the next Shift-click ranges from and must be in `shas` (or null
   * when the list is empty).
   */
  setSelection: (shas: string[], anchor: string | null) => void
  /** Publish the graph's stash lane assignment. No-ops when nothing changed. */
  setStashTracks: (tracks: Record<string, number>) => void
  /** Drop view state tied to one repo. Call when the active repo changes. */
  resetForRepoSwitch: () => void
  focusChanges: () => void
  revealRefInGraph: (name: string) => void
  revealShaInGraph: (sha: string) => void
  /** Update the toolbar search text. */
  setCommitSearch: (query: string) => void
  /** Step to the next (dir 1) or previous (dir -1) commit matching the search. */
  jumpMatch: (dir: 1 | -1) => void
  /** Ask the toolbar to focus its search box (Ctrl+F). */
  requestSearchFocus: () => void
  /** Publish the match count and current ordinal (null both = not searching). */
  setSearchMatchStatus: (count: number | null, index: number | null) => void
  openMerge: (source?: string) => void
  openNewTag: (sha?: string) => void
  openNewBranch: (sha?: string) => void
  /** Ask whether to send these local-only tags; empty list closes the prompt. */
  promptPushTags: (tags: PendingTag[]) => void
  renameBranchPrompt: (name: string | null) => void
  deleteBranchPrompt: (name: string | null) => void
  deleteRemoteBranchPrompt: (target: { remote: string; branch: string } | null) => void
  resetToBranchPrompt: (name: string | null) => void
  openRemoteSync: (source: string, target: string) => void
  /** Flip the sync direction in the open Sync modal (source <-> target). */
  swapSync: () => void
  openDiff: (request: DiffRequest) => void
  closeDiff: () => void
  openFileHistory: (path: string) => void
  openBlame: (path: string, sha?: string | null) => void
  openConflict: (path: string) => void
  showSettings: (section?: SettingsSection) => void
  showGraph: () => void
  /** Open the "Add a repository" tab and show its open/clone/new/groups picker. */
  showRepoPicker: () => void
  /** Close the "Add a repository" tab and fall back to the graph. */
  closeRepoPicker: () => void
  /** Trigger the placeholder tab's "nuh uh, over here" wiggle. */
  wiggleRepoPicker: () => void
  openGithubItem: (kind: 'pr' | 'issue', number: number) => void
  closeGithubItem: () => void
  toggleSection: (key: SectionKey) => void
  openModal: (kind: Exclude<ModalKind, null>) => void
  /** Open the remotes modal with one remote already in its edit form. */
  editRemotePrompt: (name: string) => void
  /** Open the remotes modal with one remote's delete confirm already up. */
  deleteRemotePrompt: (name: string) => void
  closeModal: () => void
  setSettingsSection: (section: SettingsSection) => void
  /** Jump to the section holding a setting and flash that row. */
  revealSettingById: (section: SettingsSection, id: string) => void
}

/**
 * Views that show one repo's contents and mean nothing once a different repo
 * is active, so switching repos drops back to the graph.
 */
const REPO_SCOPED_VIEWS = new Set<CenterView>([
  'diff',
  'conflict',
  'github',
  'fileHistory',
  'blame',
])

export const useUiStore = create<UiState>((set) => ({
  centerView: 'graph',
  selectedSha: null,
  selectedShas: [],
  diffRequest: null,
  conflictPath: null,
  fileTarget: null,
  sectionOpen: {
    local: true,
    remote: false,
    worktrees: true,
    stashes: true,
    prs: true,
    issues: false,
    tags: false,
    submodules: true,
  },
  activeModal: null,
  mergeSource: null,
  syncSource: null,
  syncTarget: null,
  tagTargetSha: null,
  branchTargetSha: null,
  tagsToPush: null,
  remoteToEdit: null,
  remoteToDelete: null,
  branchToRename: null,
  branchToDelete: null,
  remoteBranchToDelete: null,
  branchToResetTo: null,
  settingsSection: 'general',
  revealSetting: null,
  changesFocusNonce: 0,
  revealRef: null,
  revealSha: null,
  commitSearch: '',
  searchJumpNonce: 0,
  searchFocusNonce: 0,
  searchMatchCount: null,
  searchMatchIndex: null,
  githubItem: null,
  repoPickerWiggleNonce: 0,
  repoPickerOpen: false,
  stashTracks: {},
  awaitingCommitSha: null,

  selectCommit: (sha) => set({ selectedSha: sha, selectedShas: sha ? [sha] : [] }),
  commitLanding: (sha) => set({ awaitingCommitSha: sha }),
  setSelection: (shas, anchor) =>
    set({ selectedShas: shas, selectedSha: shas.length > 0 ? anchor : null }),
  setStashTracks: (tracks) =>
    set((s) => {
      const prev = s.stashTracks
      const keys = Object.keys(tracks)
      const same =
        keys.length === Object.keys(prev).length && keys.every((k) => prev[k] === tracks[k])
      return same ? {} : { stashTracks: tracks }
    }),
  resetForRepoSwitch: () =>
    set((s) => ({
      selectedSha: null,
      selectedShas: [],
      diffRequest: null,
      conflictPath: null,
      fileTarget: null,
      revealRef: null,
      revealSha: null,
      commitSearch: '',
      searchMatchCount: null,
      searchMatchIndex: null,
      githubItem: null,
      stashTracks: {},
      awaitingCommitSha: null,
      centerView: REPO_SCOPED_VIEWS.has(s.centerView) ? 'graph' : s.centerView,
    })),
  focusChanges: () => set((s) => ({ changesFocusNonce: s.changesFocusNonce + 1 })),
  revealRefInGraph: (name) =>
    set((s) => ({
      centerView: 'graph',
      diffRequest: null,
      fileTarget: null,
      revealRef: { name, nonce: (s.revealRef?.nonce ?? 0) + 1 },
    })),
  revealShaInGraph: (sha) =>
    set((s) => ({
      centerView: 'graph',
      diffRequest: null,
      fileTarget: null,
      revealSha: { sha, nonce: (s.revealSha?.nonce ?? 0) + 1 },
    })),
  setCommitSearch: (query) => set({ commitSearch: query, searchJumpNonce: 0 }),
  jumpMatch: (dir) => set((s) => ({ searchJumpNonce: s.searchJumpNonce + dir })),
  requestSearchFocus: () => set((s) => ({ searchFocusNonce: s.searchFocusNonce + 1 })),
  setSearchMatchStatus: (count, index) =>
    set({ searchMatchCount: count, searchMatchIndex: index }),
  openMerge: (source) => set({ activeModal: 'merge', mergeSource: source ?? null }),
  openNewTag: (sha) => set({ activeModal: 'newTag', tagTargetSha: sha ?? null }),
  openNewBranch: (sha) => set({ activeModal: 'newBranch', branchTargetSha: sha ?? null }),
  promptPushTags: (tags) => set({ tagsToPush: tags.length > 0 ? tags : null }),
  renameBranchPrompt: (name) => set({ branchToRename: name }),
  deleteBranchPrompt: (name) => set({ branchToDelete: name }),
  deleteRemoteBranchPrompt: (target) => set({ remoteBranchToDelete: target }),
  resetToBranchPrompt: (name) => set({ branchToResetTo: name }),
  openRemoteSync: (source, target) =>
    set({ activeModal: 'remote-sync', syncSource: source, syncTarget: target }),
  swapSync: () => set((s) => ({ syncSource: s.syncTarget, syncTarget: s.syncSource })),
  // Remember which commit a diff came from, so the file view tabs can offer
  // that commit's blame and diff rather than dropping back to the working tree.
  openDiff: (request) =>
    set({
      diffRequest: request,
      fileTarget: {
        path: request.path,
        sha: request.source.kind === 'commit' ? request.source.sha : null,
      },
      centerView: 'diff',
    }),
  closeDiff: () => set({ diffRequest: null, fileTarget: null, centerView: 'graph' }),
  // History covers the whole file rather than one commit, but the commit we
  // arrived with is kept so tabbing through History and back to Diff or Blame
  // still lands on that commit instead of the working tree.
  openFileHistory: (path) =>
    set((s) => ({
      centerView: 'fileHistory',
      fileTarget: { path, sha: s.fileTarget?.path === path ? (s.fileTarget.sha ?? null) : null },
      diffRequest: null,
    })),
  openBlame: (path, sha = null) =>
    set({ centerView: 'blame', fileTarget: { path, sha }, diffRequest: null }),
  openConflict: (path) => set({ conflictPath: path, centerView: 'conflict' }),
  showSettings: (section) =>
    set((s) => ({
      centerView: 'settings',
      diffRequest: null,
      fileTarget: null,
      settingsSection: section ?? s.settingsSection,
    })),
  showGraph: () => set({ centerView: 'graph', diffRequest: null, fileTarget: null }),
  showRepoPicker: () =>
    set({ centerView: 'repoPicker', repoPickerOpen: true, diffRequest: null, fileTarget: null }),
  closeRepoPicker: () =>
    set((s) => ({
      repoPickerOpen: false,
      centerView: s.centerView === 'repoPicker' ? 'graph' : s.centerView,
    })),
  wiggleRepoPicker: () => set((s) => ({ repoPickerWiggleNonce: s.repoPickerWiggleNonce + 1 })),
  openGithubItem: (kind, number) =>
    set({ centerView: 'github', githubItem: { kind, number }, diffRequest: null, fileTarget: null }),
  closeGithubItem: () => set({ centerView: 'graph', githubItem: null }),
  toggleSection: (key) =>
    set((s) => ({ sectionOpen: { ...s.sectionOpen, [key]: !s.sectionOpen[key] } })),
  openModal: (kind) => set({ activeModal: kind, remoteToEdit: null, remoteToDelete: null }),
  editRemotePrompt: (name) =>
    set({ activeModal: 'remotes', remoteToEdit: name, remoteToDelete: null }),
  deleteRemotePrompt: (name) =>
    set({ activeModal: 'remotes', remoteToDelete: name, remoteToEdit: null }),
  closeModal: () =>
    set({
      activeModal: null,
      syncSource: null,
      syncTarget: null,
      tagTargetSha: null,
      branchTargetSha: null,
      remoteToEdit: null,
      remoteToDelete: null,
    }),
  setSettingsSection: (section) => set({ settingsSection: section, revealSetting: null }),
  revealSettingById: (section, id) =>
    set((s) => ({
      centerView: 'settings',
      settingsSection: section,
      revealSetting: { id, nonce: (s.revealSetting?.nonce ?? 0) + 1 },
    })),
}))
