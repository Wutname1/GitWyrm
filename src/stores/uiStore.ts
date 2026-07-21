import { create } from 'zustand'
import type { DiffSource } from '@/lib/bindings'
import type { SectionKey } from '@/lib/types'

export type CenterView = 'graph' | 'diff' | 'settings' | 'conflict' | 'github'

export type ModalKind =
  | 'onboarding'
  | 'clone'
  | 'tutorial'
  | 'merge'
  | 'remote-sync'
  | 'newBranch'
  | 'newTag'
  | 'remotes'
  | 'githubConnect'
  | null

export interface GithubItemRef {
  kind: 'pr' | 'issue'
  number: number
}

export type SettingsSection = 'general' | 'ai' | 'appearance' | 'logs' | 'about'

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

interface UiState {
  centerView: CenterView
  selectedSha: string | null
  diffRequest: DiffRequest | null
  conflictPath: string | null
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
  settingsSection: SettingsSection
  changesFocusNonce: number
  /** Ref (branch/tag) the graph should scroll to and highlight; bumped nonce re-triggers. */
  revealRef: { name: string; nonce: number } | null
  /** PR or issue shown in the center view and the actions panel. */
  githubItem: GithubItemRef | null

  selectCommit: (sha: string | null) => void
  /** Drop view state tied to one repo. Call when the active repo changes. */
  resetForRepoSwitch: () => void
  focusChanges: () => void
  revealRefInGraph: (name: string) => void
  openMerge: (source?: string) => void
  openNewTag: (sha?: string) => void
  openNewBranch: (sha?: string) => void
  /** Ask whether to send these local-only tags; empty list closes the prompt. */
  promptPushTags: (tags: PendingTag[]) => void
  renameBranchPrompt: (name: string | null) => void
  deleteBranchPrompt: (name: string | null) => void
  openRemoteSync: (source: string, target: string) => void
  openDiff: (request: DiffRequest) => void
  closeDiff: () => void
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

export const useUiStore = create<UiState>((set) => ({
  centerView: 'graph',
  selectedSha: null,
  diffRequest: null,
  conflictPath: null,
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
  settingsSection: 'general',
  changesFocusNonce: 0,
  revealRef: null,
  githubItem: null,

  selectCommit: (sha) => set({ selectedSha: sha }),
  resetForRepoSwitch: () =>
    set((s) => ({
      selectedSha: null,
      diffRequest: null,
      conflictPath: null,
      revealRef: null,
      githubItem: null,
      centerView:
        s.centerView === 'diff' || s.centerView === 'conflict' || s.centerView === 'github'
          ? 'graph'
          : s.centerView,
    })),
  focusChanges: () => set((s) => ({ changesFocusNonce: s.changesFocusNonce + 1 })),
  revealRefInGraph: (name) =>
    set((s) => ({
      centerView: 'graph',
      diffRequest: null,
      revealRef: { name, nonce: (s.revealRef?.nonce ?? 0) + 1 },
    })),
  openMerge: (source) => set({ activeModal: 'merge', mergeSource: source ?? null }),
  openNewTag: (sha) => set({ activeModal: 'newTag', tagTargetSha: sha ?? null }),
  openNewBranch: (sha) => set({ activeModal: 'newBranch', branchTargetSha: sha ?? null }),
  promptPushTags: (tags) => set({ tagsToPush: tags.length > 0 ? tags : null }),
  renameBranchPrompt: (name) => set({ branchToRename: name }),
  deleteBranchPrompt: (name) => set({ branchToDelete: name }),
  openRemoteSync: (source, target) =>
    set({ activeModal: 'remote-sync', syncSource: source, syncTarget: target }),
  openDiff: (request) => set({ diffRequest: request, centerView: 'diff' }),
  closeDiff: () => set({ diffRequest: null, centerView: 'graph' }),
  openConflict: (path) => set({ conflictPath: path, centerView: 'conflict' }),
  showSettings: (section) =>
    set((s) => ({
      centerView: 'settings',
      diffRequest: null,
      settingsSection: section ?? s.settingsSection,
    })),
  showGraph: () => set({ centerView: 'graph', diffRequest: null }),
  openGithubItem: (kind, number) =>
    set({ centerView: 'github', githubItem: { kind, number }, diffRequest: null }),
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
