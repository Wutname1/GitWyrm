import { create } from 'zustand'
import type { DiffSource } from '@/lib/bindings'
import type { SectionKey } from '@/lib/types'

export type CenterView = 'graph' | 'diff' | 'settings' | 'conflict'

export type ModalKind =
  | 'onboarding'
  | 'clone'
  | 'tutorial'
  | 'merge'
  | 'remote-sync'
  | 'newBranch'
  | 'newTag'
  | 'remotes'
  | null

export type SettingsSection = 'general' | 'ai' | 'appearance' | 'logs' | 'about'

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
  settingsSection: SettingsSection
  changesFocusNonce: number
  /** Ref (branch/tag) the graph should scroll to and highlight; bumped nonce re-triggers. */
  revealRef: { name: string; nonce: number } | null

  selectCommit: (sha: string | null) => void
  focusChanges: () => void
  revealRefInGraph: (name: string) => void
  openMerge: (source?: string) => void
  openNewTag: (sha?: string) => void
  openNewBranch: (sha?: string) => void
  openRemoteSync: (source: string, target: string) => void
  openDiff: (request: DiffRequest) => void
  closeDiff: () => void
  openConflict: (path: string) => void
  showSettings: (section?: SettingsSection) => void
  showGraph: () => void
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
  settingsSection: 'general',
  changesFocusNonce: 0,
  revealRef: null,

  selectCommit: (sha) => set({ selectedSha: sha }),
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
