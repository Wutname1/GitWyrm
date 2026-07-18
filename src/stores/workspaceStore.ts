import { create } from 'zustand'
import { commands, type BranchSwitchMode, type RepoInfo, type Settings } from '@/lib/bindings'
import { normalizePath } from '@/lib/paths'
import { unwrap } from '@/lib/queryKeys'
import { DEFAULT_COLUMN_ORDER, type ColumnId } from '@/lib/graphColumns'

export interface RecentRepo {
  name: string
  path: string
}

export type UpdateChannel = 'stable' | 'beta'
export type CommitButtonMode = 'commit' | 'commit_push'
export type { BranchSwitchMode }

/** Whole-app zoom limits and step, shared by the store and the status-bar control. */
export const MIN_UI_SCALE = 0.5
export const MAX_UI_SCALE = 2.0
export const UI_SCALE_STEP = 0.1
export const DEFAULT_UI_SCALE = 1.0

/** Clamps a scale into the supported range and rounds to whole percent. */
export function clampUiScale(scale: number): number {
  if (!Number.isFinite(scale)) return DEFAULT_UI_SCALE
  const clamped = Math.min(MAX_UI_SCALE, Math.max(MIN_UI_SCALE, scale))
  return Math.round(clamped * 100) / 100
}

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
  /** What to do with uncommitted changes when switching branches (persisted). */
  branchSwitchMode: BranchSwitchMode
  /** AI provider id used for commit message generation (persisted). */
  aiProvider: string | null
  /** Model id within the selected AI provider (persisted). */
  aiModel: string | null
  /** Custom commit-generation instruction; null uses the built-in default (persisted). */
  aiInstruction: string | null
  /** Commit-graph column order (persisted). */
  columnOrder: ColumnId[]
  /** Commit-graph columns the user has hidden (persisted). */
  hiddenColumns: ColumnId[]
  /** Default action for the commit button (persisted). */
  commitButtonMode: CommitButtonMode
  /** Show worktree actions and the worktree sidebar section (persisted). */
  enableWorktrees: boolean
  /** Whole-app zoom factor, 1.0 = 100% (persisted). */
  uiScale: number
  /** Custom tab names, keyed by repo path. Missing paths use the repo folder name (persisted). */
  tabAliases: Record<string, string>
  /** True once settings.json has been read on launch. */
  hydrated: boolean

  addRepo: (repo: RepoInfo) => void
  removeRepo: (id: string) => void
  setActiveRepo: (id: string) => void
  setCodeFolder: (path: string | null) => void
  setCloneDirectory: (path: string | null) => void
  setUpdateChannel: (channel: UpdateChannel) => void
  setBranchSwitchMode: (mode: BranchSwitchMode) => void
  setAiSelection: (provider: string | null, model: string | null) => void
  setAiInstruction: (instruction: string | null) => void
  setCommitButtonMode: (mode: CommitButtonMode) => void
  setEnableWorktrees: (enabled: boolean) => void
  /** Set the whole-app zoom factor (clamped to the supported range). */
  setUiScale: (scale: number) => void
  /** Rename a tab by repo path. An empty/blank alias clears it (back to the folder name). */
  setTabAlias: (path: string, alias: string) => void
  /** Move a column to a new index in the display order. */
  reorderColumn: (id: ColumnId, toIndex: number) => void
  /** Show or hide a column. */
  toggleColumn: (id: ColumnId) => void
  /** Restore the default column order and show every column. */
  resetColumns: () => void
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
    branch_switch_mode: s.branchSwitchMode,
    ai_provider: s.aiProvider,
    ai_model: s.aiModel,
    ai_instruction: s.aiInstruction,
    column_layout: { order: s.columnOrder, hidden: s.hiddenColumns },
    commit_button_mode: s.commitButtonMode,
    enable_worktrees: s.enableWorktrees,
    ui_scale: s.uiScale,
    tab_aliases: s.tabAliases,
  }
}

/** Column ids known to this build; anything else in persisted layout is dropped. */
const KNOWN_COLUMNS = new Set<ColumnId>(DEFAULT_COLUMN_ORDER)

/**
 * Sanitizes a persisted order: keeps only known ids, drops duplicates, and
 * appends any columns missing from the saved order (e.g. added in a new build)
 * so every column stays reachable.
 */
function normalizeOrder(order: string[] | undefined): ColumnId[] {
  const seen = new Set<ColumnId>()
  const result: ColumnId[] = []
  for (const id of order ?? []) {
    if (isColumnId(id) && !seen.has(id)) {
      seen.add(id)
      result.push(id)
    }
  }
  for (const id of DEFAULT_COLUMN_ORDER) {
    if (!seen.has(id)) result.push(id)
  }
  return result
}

function normalizeHidden(hidden: string[] | undefined): ColumnId[] {
  return (hidden ?? []).filter(isColumnId)
}

function isColumnId(id: string): id is ColumnId {
  return KNOWN_COLUMNS.has(id as ColumnId)
}

/**
 * Settings map values arrive as `string | undefined` (a Rust HashMap maps to a
 * Partial record). Drop the empty entries so the store holds a plain map.
 */
function normalizeAliases(aliases: Partial<Record<string, string>> | undefined): Record<string, string> {
  const out: Record<string, string> = {}
  for (const [path, alias] of Object.entries(aliases ?? {})) {
    if (alias) out[path] = alias
  }
  return out
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
  branchSwitchMode: 'auto_stash',
  aiProvider: null,
  aiModel: null,
  aiInstruction: null,
  columnOrder: DEFAULT_COLUMN_ORDER,
  hiddenColumns: [],
  commitButtonMode: 'commit',
  enableWorktrees: false,
  uiScale: DEFAULT_UI_SCALE,
  tabAliases: {},
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
  setBranchSwitchMode: (mode) => {
    set({ branchSwitchMode: mode })
    schedulePersist()
  },
  setAiSelection: (provider, model) => {
    set({ aiProvider: provider, aiModel: model })
    schedulePersist()
  },
  setAiInstruction: (instruction) => {
    set({ aiInstruction: instruction })
    schedulePersist()
  },
  setCommitButtonMode: (mode) => {
    set({ commitButtonMode: mode })
    schedulePersist()
  },
  setEnableWorktrees: (enabled) => {
    set({ enableWorktrees: enabled })
    schedulePersist()
  },
  setUiScale: (scale) => {
    set({ uiScale: clampUiScale(scale) })
    schedulePersist()
  },
  setTabAlias: (path, alias) => {
    set((s) => {
      const next = { ...s.tabAliases }
      const trimmed = alias.trim()
      if (trimmed) next[path] = trimmed
      else delete next[path]
      return { tabAliases: next }
    })
    schedulePersist()
  },
  reorderColumn: (id, toIndex) => {
    set((s) => {
      const from = s.columnOrder.indexOf(id)
      if (from === -1) return s
      const next = [...s.columnOrder]
      next.splice(from, 1)
      next.splice(Math.max(0, Math.min(toIndex, next.length)), 0, id)
      return { columnOrder: next }
    })
    schedulePersist()
  },
  toggleColumn: (id) => {
    set((s) => ({
      hiddenColumns: s.hiddenColumns.includes(id)
        ? s.hiddenColumns.filter((c) => c !== id)
        : [...s.hiddenColumns, id],
    }))
    schedulePersist()
  },
  resetColumns: () => {
    set({ columnOrder: DEFAULT_COLUMN_ORDER, hiddenColumns: [] })
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
        branchSwitchMode: settings.branch_switch_mode ?? 'auto_stash',
        aiProvider: settings.ai_provider ?? null,
        aiModel: settings.ai_model ?? null,
        aiInstruction: settings.ai_instruction ?? null,
        columnOrder: normalizeOrder(settings.column_layout?.order),
        hiddenColumns: normalizeHidden(settings.column_layout?.hidden),
        commitButtonMode: settings.commit_button_mode === 'commit_push' ? 'commit_push' : 'commit',
        enableWorktrees: settings.enable_worktrees ?? false,
        uiScale: settings.ui_scale != null ? clampUiScale(settings.ui_scale) : DEFAULT_UI_SCALE,
        tabAliases: normalizeAliases(settings.tab_aliases),
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
