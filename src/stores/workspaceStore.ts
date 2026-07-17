import { create } from 'zustand'
import { commands, type RepoInfo, type Settings } from '@/lib/bindings'
import { normalizePath } from '@/lib/paths'
import { unwrap } from '@/lib/queryKeys'

export interface RecentRepo {
  name: string
  path: string
}

export type UpdateChannel = 'stable' | 'beta'

interface WorkspaceState {
  /** Repos currently open in tabs. */
  openRepos: RepoInfo[]
  /** Active repo id (tab). */
  activeRepoId: string | null
  /** Recently opened repo paths, most recent first (persisted). */
  recents: RecentRepo[]
  /** User-selected code folder scanned for quick-launch repos (persisted). */
  codeFolder: string | null
  /** Default directory new clones go into (persisted; falls back to codeFolder). */
  cloneDirectory: string | null
  /** Release channel used when checking for updates (persisted). */
  updateChannel: UpdateChannel
  /** AI provider id used for commit message generation (persisted). */
  aiProvider: string | null
  /** Model id within the selected AI provider (persisted). */
  aiModel: string | null
  /** True once settings.json has been read on launch. */
  hydrated: boolean

  addRepo: (repo: RepoInfo) => void
  removeRepo: (id: string) => void
  setActiveRepo: (id: string) => void
  setCodeFolder: (path: string | null) => void
  setCloneDirectory: (path: string | null) => void
  setUpdateChannel: (channel: UpdateChannel) => void
  setAiSelection: (provider: string | null, model: string | null) => void
  /** Reads settings.json once and hydrates the store; returns the raw settings for launch-time restore. */
  hydrate: () => Promise<Settings>
}

/** Fields persisted to settings.json (excludes in-memory-only state like openRepos handles). */
function toSettings(s: WorkspaceState): Settings {
  return {
    open_repos: s.openRepos.map((r) => r.path),
    active_repo_path: s.openRepos.find((r) => r.id === s.activeRepoId)?.path ?? null,
    recents: s.recents.map((r) => ({ name: r.name, path: r.path })),
    code_folder: s.codeFolder,
    clone_directory: s.cloneDirectory,
    update_channel: s.updateChannel === 'beta' ? 'beta' : 'stable',
    ai_provider: s.aiProvider,
    ai_model: s.aiModel,
  }
}

let persistTimer: ReturnType<typeof setTimeout> | null = null

/** Debounced write-through to settings.json; skipped until hydration completes. */
function schedulePersist() {
  const s = useWorkspaceStore.getState()
  if (!s.hydrated) return
  if (persistTimer) clearTimeout(persistTimer)
  persistTimer = setTimeout(() => {
    void commands.saveSettings(toSettings(useWorkspaceStore.getState()))
  }, 300)
}

export const useWorkspaceStore = create<WorkspaceState>()((set, get) => ({
  openRepos: [],
  activeRepoId: null,
  recents: [],
  codeFolder: null,
  cloneDirectory: null,
  updateChannel: 'stable',
  aiProvider: null,
  aiModel: null,
  hydrated: false,

  addRepo: (repo) => {
    set((s) => {
      const openRepos = s.openRepos.some((r) => r.id === repo.id)
        ? s.openRepos
        : [...s.openRepos, repo]
      const recents = [
        { name: repo.name, path: repo.path },
        ...s.recents.filter((r) => r.path !== repo.path),
      ].slice(0, 10)
      return { openRepos, activeRepoId: repo.id, recents }
    })
    schedulePersist()
  },
  removeRepo: (id) => {
    set((s) => {
      const openRepos = s.openRepos.filter((r) => r.id !== id)
      const activeRepoId =
        s.activeRepoId === id ? (openRepos[0]?.id ?? null) : s.activeRepoId
      return { openRepos, activeRepoId }
    })
    schedulePersist()
  },
  setActiveRepo: (id) => {
    set({ activeRepoId: id })
    schedulePersist()
  },
  setCodeFolder: (path) => {
    set({ codeFolder: path ? normalizePath(path) : null })
    schedulePersist()
  },
  setCloneDirectory: (path) => {
    set({ cloneDirectory: path ? normalizePath(path) : null })
    schedulePersist()
  },
  setUpdateChannel: (channel) => {
    set({ updateChannel: channel })
    schedulePersist()
  },
  setAiSelection: (provider, model) => {
    set({ aiProvider: provider, aiModel: model })
    schedulePersist()
  },

  hydrate: async () => {
    const settings = unwrap(await commands.getSettings())
    if (!get().hydrated) {
      set({
        recents: settings.recents ?? [],
        codeFolder: settings.code_folder ?? null,
        cloneDirectory: settings.clone_directory ?? null,
        updateChannel: settings.update_channel === 'beta' ? 'beta' : 'stable',
        aiProvider: settings.ai_provider ?? null,
        aiModel: settings.ai_model ?? null,
        hydrated: true,
      })
    }
    return settings
  },
}))

export function useActiveRepo(): RepoInfo | null {
  const openRepos = useWorkspaceStore((s) => s.openRepos)
  const activeRepoId = useWorkspaceStore((s) => s.activeRepoId)
  return openRepos.find((r) => r.id === activeRepoId) ?? null
}
