import { create } from 'zustand'
import type { DiffSource } from '@/lib/bindings'
import type { SectionKey } from '@/lib/types'

export type CenterView = 'graph' | 'diff' | 'settings'

export type ModalKind = 'onboarding' | 'clone' | 'tutorial' | null

export type SettingsSection = 'general' | 'ai' | 'appearance' | 'logs' | 'about'

export interface DiffRequest {
  path: string
  source: DiffSource
}

interface UiState {
  centerView: CenterView
  selectedSha: string | null
  diffRequest: DiffRequest | null
  sectionOpen: Record<SectionKey, boolean>
  activeModal: ModalKind
  settingsSection: SettingsSection

  selectCommit: (sha: string | null) => void
  openDiff: (request: DiffRequest) => void
  closeDiff: () => void
  showSettings: () => void
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
  settingsSection: 'general',

  selectCommit: (sha) => set({ selectedSha: sha }),
  openDiff: (request) => set({ diffRequest: request, centerView: 'diff' }),
  closeDiff: () => set({ diffRequest: null, centerView: 'graph' }),
  showSettings: () => set({ centerView: 'settings', diffRequest: null }),
  showGraph: () => set({ centerView: 'graph', diffRequest: null }),
  toggleSection: (key) =>
    set((s) => ({ sectionOpen: { ...s.sectionOpen, [key]: !s.sectionOpen[key] } })),
  openModal: (kind) => set({ activeModal: kind }),
  closeModal: () => set({ activeModal: null }),
  setSettingsSection: (section) => set({ settingsSection: section }),
}))
