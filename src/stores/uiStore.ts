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

export type ModalKind =
  | 'onboarding'
  | 'clone'
  | 'tutorial'
  | 'merge'
  | 'remote-sync'
  | 'push-choice'
  | 'newBranch'
  | 'newTag'
  | 'remotes'
  | 'githubConnect'
  | null

export interface GithubItemRef {
  kind: 'pr' | 'issue'
  number: number
}

export type SettingsSection = 'general' | 'repository' | 'tags' | 'ai' | 'appearance' | 'logs' | 'about'

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
  /** Branch pending a rename / delete confirm, set from any branch menu. */
  branchToRename: string | null
  branchToDelete: string | null
  /**
   * Branch the current branch will be hard-reset to, pending confirm. Set from
   * any branch menu or a branch-onto-branch drop; the target names where the
   * checked-out branch will be rewound to.
   */
  branchToResetTo: string | null
  settingsSection: SettingsSection
  changesFocusNonce: number
  /** Ref (branch/tag) the graph should scroll to and highlight; bumped nonce re-triggers. */
  revealRef: { name: string; nonce: number } | null
  /** Commit or stash sha the graph should scroll to and select; bumped nonce re-triggers. */
  revealSha: { sha: string; nonce: number } | null
  /** PR or issue shown in the center view and the actions panel. */
  githubItem: GithubItemRef | null

  selectCommit: (sha: string | null) => void
  /** Drop view state tied to one repo. Call when the active repo changes. */
  resetForRepoSwitch: () => void
  focusChanges: () => void
  revealRefInGraph: (name: string) => void
  revealShaInGraph: (sha: string) => void
  openMerge: (source?: string) => void
  openNewTag: (sha?: string) => void
  openNewBranch: (sha?: string) => void
  /** Ask whether to send these local-only tags; empty list closes the prompt. */
  promptPushTags: (tags: PendingTag[]) => void
  renameBranchPrompt: (name: string | null) => void
  deleteBranchPrompt: (name: string | null) => void
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
  openGithubItem: (kind: 'pr' | 'issue', number: number) => void
  closeGithubItem: () => void
  toggleSection: (key: SectionKey) => void
  openModal: (kind: Exclude<ModalKind, null>) => void
  closeModal: () => void
  setSettingsSection: (section: SettingsSection) => void
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
  },
  activeModal: null,
  mergeSource: null,
  syncSource: null,
  syncTarget: null,
  tagTargetSha: null,
  branchTargetSha: null,
  tagsToPush: null,
  branchToRename: null,
  branchToDelete: null,
  branchToResetTo: null,
  settingsSection: 'general',
  changesFocusNonce: 0,
  revealRef: null,
  revealSha: null,
  githubItem: null,

  selectCommit: (sha) => set({ selectedSha: sha }),
  resetForRepoSwitch: () =>
    set((s) => ({
      selectedSha: null,
      diffRequest: null,
      conflictPath: null,
      fileTarget: null,
      revealRef: null,
      revealSha: null,
      githubItem: null,
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
  openMerge: (source) => set({ activeModal: 'merge', mergeSource: source ?? null }),
  openNewTag: (sha) => set({ activeModal: 'newTag', tagTargetSha: sha ?? null }),
  openNewBranch: (sha) => set({ activeModal: 'newBranch', branchTargetSha: sha ?? null }),
  promptPushTags: (tags) => set({ tagsToPush: tags.length > 0 ? tags : null }),
  renameBranchPrompt: (name) => set({ branchToRename: name }),
  deleteBranchPrompt: (name) => set({ branchToDelete: name }),
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
  openGithubItem: (kind, number) =>
    set({ centerView: 'github', githubItem: { kind, number }, diffRequest: null, fileTarget: null }),
  closeGithubItem: () => set({ centerView: 'graph', githubItem: null }),
  toggleSection: (key) =>
    set((s) => ({ sectionOpen: { ...s.sectionOpen, [key]: !s.sectionOpen[key] } })),
  openModal: (kind) => set({ activeModal: kind }),
  closeModal: () =>
    set({
      activeModal: null,
      syncSource: null,
      syncTarget: null,
      tagTargetSha: null,
      branchTargetSha: null,
    }),
  setSettingsSection: (section) => set({ settingsSection: section }),
}))
