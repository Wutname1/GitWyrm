import { create } from 'zustand'
import { commands, type BranchSwitchMode, type RepoInfo, type Settings } from '@/lib/bindings'
import { log } from '@/lib/log'
import { normalizePath } from '@/lib/paths'
import { unwrap } from '@/lib/queryKeys'
import {
  DEFAULT_COLUMN_ORDER,
  clampColumnWidth,
  normalizeColumnWidths,
  type ColumnId,
  type ColumnWidths,
} from '@/lib/graphColumns'
import { useUiStore } from '@/stores/uiStore'

export interface RecentRepo {
  name: string
  path: string
}

export type UpdateChannel = 'stable' | 'beta'
export type CommitButtonMode = 'commit' | 'commit_push'
export type ChangeSizeDisplay = 'row' | 'column'

/** What to do about local-only tags after a push. */
export type TagPushDefault = 'ask' | 'always' | 'never'
export type TabLayout = 'horizontal' | 'vertical'
export type TabDropPlacement = 'before' | 'after'

export interface TabGroup {
  id: string
  name: string
  color: string
  collapsed: boolean
  repoPaths: string[]
}

export interface SavedTabGroup {
  id: string
  name: string
  color: string
  repoPaths: string[]
}

export type TabOrderItem =
  | { type: 'repo'; path: string }
  | { type: 'group'; id: string }

export const TAB_GROUP_COLORS = ['#2dd4a7', '#38bdf8', '#a78bfa', '#f59e0b', '#f472b6', '#f87171'] as const
export type { BranchSwitchMode }

/** Whole-app zoom limits and step, shared by the store and the status-bar control. */
export const MIN_UI_SCALE = 0.5
export const MAX_UI_SCALE = 2.0
export const UI_SCALE_STEP = 0.1
export const DEFAULT_UI_SCALE = 1.0

/** Saved width limits for the vertical repository rail. */
export const MIN_VERTICAL_TAB_WIDTH = 48
export const MAX_VERTICAL_TAB_WIDTH = 420
export const DEFAULT_VERTICAL_TAB_WIDTH = 248

/** Saved width limits for the main workspace panes. */
export const MIN_LEFT_PANEL_WIDTH = 176
export const MAX_LEFT_PANEL_WIDTH = 420
export const DEFAULT_LEFT_PANEL_WIDTH = 240
export const MIN_RIGHT_PANEL_WIDTH = 240
export const MAX_RIGHT_PANEL_WIDTH = 520
export const DEFAULT_RIGHT_PANEL_WIDTH = 320

/** Clamps a scale into the supported range and rounds to whole percent. */
export function clampUiScale(scale: number): number {
  if (!Number.isFinite(scale)) return DEFAULT_UI_SCALE
  const clamped = Math.min(MAX_UI_SCALE, Math.max(MIN_UI_SCALE, scale))
  return Math.round(clamped * 100) / 100
}

export function clampVerticalTabWidth(width: number): number {
  if (!Number.isFinite(width)) return DEFAULT_VERTICAL_TAB_WIDTH
  return Math.round(Math.min(MAX_VERTICAL_TAB_WIDTH, Math.max(MIN_VERTICAL_TAB_WIDTH, width)))
}

export function clampLeftPanelWidth(width: number): number {
  if (!Number.isFinite(width)) return DEFAULT_LEFT_PANEL_WIDTH
  return Math.round(Math.min(MAX_LEFT_PANEL_WIDTH, Math.max(MIN_LEFT_PANEL_WIDTH, width)))
}

export function clampRightPanelWidth(width: number): number {
  if (!Number.isFinite(width)) return DEFAULT_RIGHT_PANEL_WIDTH
  return Math.round(Math.min(MAX_RIGHT_PANEL_WIDTH, Math.max(MIN_RIGHT_PANEL_WIDTH, width)))
}

function pathKey(path: string): string {
  return normalizePath(path).toLowerCase()
}

function samePath(left: string, right: string): boolean {
  return pathKey(left) === pathKey(right)
}

function groupMarker(groupId: string): string {
  return `group:${groupId}`
}

function groupForPath(groups: TabGroup[], path: string): TabGroup | undefined {
  return groups.find((group) => group.repoPaths.some((candidate) => samePath(candidate, path)))
}

function orderedRepoPaths(state: Pick<WorkspaceState, 'tabGroups' | 'tabOrder'>): string[] {
  return state.tabOrder.flatMap((item) => {
    if (item.type === 'repo') return [item.path]
    return state.tabGroups.find((group) => group.id === item.id)?.repoPaths ?? []
  })
}

function serializeTabOrder(order: TabOrderItem[]): string[] {
  return order.map((item) => item.type === 'group' ? groupMarker(item.id) : item.path)
}

interface StoredTabGroup {
  id: string
  name: string
  color: string
  collapsed?: boolean
  repo_paths?: string[]
}

function deserializeTabGroups(groups: StoredTabGroup[] | undefined): TabGroup[] {
  const seen = new Set<string>()
  const result: TabGroup[] = []
  for (const group of groups ?? []) {
    if (!group.id || seen.has(group.id)) continue
    const repoPaths = [...new Map((group.repo_paths ?? []).map((path) => [pathKey(path), normalizePath(path)])).values()]
    if (repoPaths.length === 0) continue
    seen.add(group.id)
    result.push({
      id: group.id,
      name: group.name.trim() || 'New group',
      color: group.color || TAB_GROUP_COLORS[0],
      collapsed: group.collapsed ?? false,
      repoPaths,
    })
  }
  return result
}

function deserializeSavedTabGroups(groups: StoredTabGroup[] | undefined): SavedTabGroup[] {
  return deserializeTabGroups(groups).map(({ id, name, color, repoPaths }) => ({ id, name, color, repoPaths }))
}

function deserializeTabOrder(order: string[] | undefined, groups: TabGroup[]): TabOrderItem[] {
  const result: TabOrderItem[] = []
  const seenGroups = new Set<string>()
  const seenRepos = new Set<string>()
  const groupedPaths = new Set(groups.flatMap((group) => group.repoPaths.map(pathKey)))
  const groupIds = new Set(groups.map((group) => group.id))

  for (const value of order ?? []) {
    if (value.startsWith('group:')) {
      const id = value.slice('group:'.length)
      if (groupIds.has(id) && !seenGroups.has(id)) {
        seenGroups.add(id)
        result.push({ type: 'group', id })
      }
      continue
    }
    const normalized = normalizePath(value)
    const key = pathKey(normalized)
    if (!groupedPaths.has(key) && !seenRepos.has(key)) {
      seenRepos.add(key)
      result.push({ type: 'repo', path: normalized })
    }
  }
  for (const group of groups) {
    if (!seenGroups.has(group.id)) result.push({ type: 'group', id: group.id })
  }
  return result
}

function removePathFromWorkspace(
  groups: TabGroup[],
  order: TabOrderItem[],
  repoPath: string,
): { groups: TabGroup[]; order: TabOrderItem[] } {
  const nextGroups = groups
    .map((group) => ({ ...group, repoPaths: group.repoPaths.filter((path) => !samePath(path, repoPath)) }))
    .filter((group) => group.repoPaths.length > 0)
  const survivingGroups = new Set(nextGroups.map((group) => group.id))
  const nextOrder = order.filter((item) => {
    if (item.type === 'repo') return !samePath(item.path, repoPath)
    return survivingGroups.has(item.id)
  })
  return { groups: nextGroups, order: nextOrder }
}

function workspaceIndexForPath(groups: TabGroup[], order: TabOrderItem[], repoPath: string): number {
  const group = groupForPath(groups, repoPath)
  return order.findIndex((item) => group
    ? item.type === 'group' && item.id === group.id
    : item.type === 'repo' && samePath(item.path, repoPath))
}

function newGroupId(): string {
  return `group-${Date.now()}-${Math.random().toString(36).slice(2, 7)}`
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
  /** Explicit commit-graph column widths (persisted). */
  columnWidths: ColumnWidths
  /** Width of the branches and tags pane (persisted). */
  leftPanelWidth: number
  /** Width of the changes and commit pane (persisted). */
  rightPanelWidth: number
  /** Where change size appears in the commit graph (persisted). */
  changeSizeDisplay: ChangeSizeDisplay
  /** Whether commit rows show a change-size indicator (persisted). */
  showChangeIndicator: boolean
  /** Whether the change-size indicator includes exact line counts (persisted). */
  showChangeLineCounts: boolean
  /** Default action for the commit button (persisted). */
  commitButtonMode: CommitButtonMode
  /** Whether a push offers to send local-only tags too (persisted). */
  tagPushDefault: TagPushDefault
  /** Whether the New Tag dialog's send-to-remote box starts checked (persisted). */
  tagPushOnCreate: boolean
  /** Show worktree actions and the worktree sidebar section (persisted). */
  enableWorktrees: boolean
  /** Whole-app zoom factor, 1.0 = 100% (persisted). */
  uiScale: number
  /** Custom tab names, keyed by repo path. Missing paths use the repo folder name (persisted). */
  tabAliases: Record<string, string>
  /** Show favicon or logo images in repository tabs (persisted). */
  showRepoIcons: boolean
  /** Hide repository names until a tab is hovered (persisted). */
  tabIconOnly: boolean
  /** Width of the vertical repository rail in pixels (persisted). */
  verticalTabWidth: number
  /** In-memory change counters used to refresh an icon everywhere it is shown. */
  repoIconRevisions: Record<string, number>
  /** Whether repository tabs run across the top or down the left side (persisted). */
  tabLayout: TabLayout
  /** Give horizontal tabs their own row under the app bar instead of sharing it (persisted). */
  horizontalTabRow: boolean
  /** Groups that currently wrap open repository tabs (persisted while open). */
  tabGroups: TabGroup[]
  /** Shared order of loose repository tabs and complete groups (persisted). */
  tabOrder: TabOrderItem[]
  /** Reusable group snapshots available from the repository picker (persisted). */
  savedTabGroups: SavedTabGroup[]
  /** True once settings.json has been read on launch. */
  hydrated: boolean

  addRepo: (repo: RepoInfo) => void
  /** Opens several repos as tabs at once without changing which tab is active. */
  addReposInBackground: (repos: RepoInfo[]) => void
  removeRepo: (id: string) => void
  setActiveRepo: (id: string) => void
  setCodeFolder: (path: string | null) => void
  setCloneDirectory: (path: string | null) => void
  setUpdateChannel: (channel: UpdateChannel) => void
  setBranchSwitchMode: (mode: BranchSwitchMode) => void
  setAiSelection: (provider: string | null, model: string | null) => void
  setAiInstruction: (instruction: string | null) => void
  setCommitButtonMode: (mode: CommitButtonMode) => void
  setTagPushDefault: (mode: TagPushDefault) => void
  setTagPushOnCreate: (enabled: boolean) => void
  setEnableWorktrees: (enabled: boolean) => void
  setTabLayout: (layout: TabLayout) => void
  setHorizontalTabRow: (enabled: boolean) => void
  /** Set the whole-app zoom factor (clamped to the supported range). */
  setUiScale: (scale: number) => void
  /** Rename a tab by repo path. An empty/blank alias clears it (back to the folder name). */
  setTabAlias: (path: string, alias: string) => void
  setShowRepoIcons: (enabled: boolean) => void
  setTabIconOnly: (enabled: boolean) => void
  setVerticalTabWidth: (width: number) => void
  refreshRepoIcon: (path: string) => void
  createTabGroup: (repoPaths: string[], options?: { name?: string; color?: string; id?: string }) => string
  addRepoToGroup: (repoPath: string, groupId: string) => void
  removeRepoFromGroup: (repoPath: string) => void
  renameTabGroup: (groupId: string, name: string) => void
  setTabGroupColor: (groupId: string, color: string) => void
  toggleTabGroup: (groupId: string) => void
  ungroupTabGroup: (groupId: string) => void
  removeTabGroup: (groupId: string) => void
  saveTabGroup: (groupId: string) => void
  createSavedTabGroup: (repoPaths: string[], name: string) => string | null
  deleteSavedTabGroup: (groupId: string) => void
  moveRepoBeside: (sourcePath: string, targetPath: string, placement: TabDropPlacement) => void
  moveRepoToOrder: (repoPath: string, orderIndex: number) => void
  moveGroupToOrder: (groupId: string, orderIndex: number) => void
  /** Removes paths that failed to reopen after launch. */
  finishRepoRestore: () => void
  /** Move a column to a new index in the display order. */
  reorderColumn: (id: ColumnId, toIndex: number) => void
  /** Show or hide a column. */
  toggleColumn: (id: ColumnId) => void
  /** Restore the default column order and show every column. */
  resetColumns: () => void
  /** Resize one graph column. */
  setColumnWidth: (id: ColumnId, width: number) => void
  /** Return one graph column to its default sizing behavior. */
  resetColumnWidth: (id: ColumnId) => void
  setLeftPanelWidth: (width: number) => void
  setRightPanelWidth: (width: number) => void
  setChangeSizeDisplay: (display: ChangeSizeDisplay) => void
  setShowChangeIndicator: (enabled: boolean) => void
  setShowChangeLineCounts: (enabled: boolean) => void
  /** Reads settings.json once and hydrates the store; returns the raw settings for launch-time restore. */
  hydrate: () => Promise<Settings>
}

/** Fields persisted to settings.json (excludes in-memory-only state like openRepos handles). */
function toSettings(s: WorkspaceState): Settings {
  return {
    open_repos: orderedRepoPaths(s).filter((path) => s.openRepos.some((repo) => samePath(repo.path, path))),
    active_repo_path: s.openRepos.find((r) => r.id === s.activeRepoId)?.path ?? null,
    recents: s.recents.map((r) => ({ name: r.name, path: r.path })),
    code_folder: s.codeFolder,
    clone_directory: s.cloneDirectory,
    update_channel: s.updateChannel === 'beta' ? 'beta' : 'stable',
    branch_switch_mode: s.branchSwitchMode,
    ai_provider: s.aiProvider,
    ai_model: s.aiModel,
    ai_instruction: s.aiInstruction,
    column_layout: { order: s.columnOrder, hidden: s.hiddenColumns, widths: s.columnWidths },
    left_panel_width: s.leftPanelWidth,
    right_panel_width: s.rightPanelWidth,
    change_size_display: s.changeSizeDisplay,
    show_change_indicator: s.showChangeIndicator,
    show_change_line_counts: s.showChangeLineCounts,
    commit_button_mode: s.commitButtonMode,
    tag_push_default: s.tagPushDefault,
    tag_push_on_create: s.tagPushOnCreate,
    enable_worktrees: s.enableWorktrees,
    ui_scale: s.uiScale,
    tab_aliases: s.tabAliases,
    show_repo_icons: s.showRepoIcons,
    tab_icon_only: s.tabIconOnly,
    vertical_tab_width: s.verticalTabWidth,
    tab_layout: s.tabLayout,
    horizontal_tab_row: s.horizontalTabRow,
    tab_groups: s.tabGroups.map((group) => ({
      id: group.id,
      name: group.name,
      color: group.color,
      collapsed: group.collapsed,
      repo_paths: group.repoPaths,
    })),
    tab_order: serializeTabOrder(s.tabOrder),
    saved_tab_groups: s.savedTabGroups.map((group) => ({
      id: group.id,
      name: group.name,
      color: group.color,
      collapsed: false,
      repo_paths: group.repoPaths,
    })),
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
  // Older settings have no Changes column. Place it beside Author, matching
  // the new default, while preserving every other saved column position.
  if (!seen.has('changes')) {
    const authorIndex = result.indexOf('author')
    if (authorIndex >= 0) {
      result.splice(authorIndex + 1, 0, 'changes')
      seen.add('changes')
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

/** Unknown or missing values fall back to asking, the safest of the three. */
function normalizeTagPushDefault(mode: string | null | undefined): TagPushDefault {
  return mode === 'always' || mode === 'never' ? mode : 'ask'
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
  columnWidths: {},
  leftPanelWidth: DEFAULT_LEFT_PANEL_WIDTH,
  rightPanelWidth: DEFAULT_RIGHT_PANEL_WIDTH,
  changeSizeDisplay: 'column',
  showChangeIndicator: true,
  showChangeLineCounts: false,
  commitButtonMode: 'commit',
  tagPushDefault: 'ask',
  tagPushOnCreate: false,
  enableWorktrees: false,
  uiScale: DEFAULT_UI_SCALE,
  tabAliases: {},
  showRepoIcons: true,
  tabIconOnly: false,
  verticalTabWidth: DEFAULT_VERTICAL_TAB_WIDTH,
  repoIconRevisions: {},
  tabLayout: 'horizontal',
  horizontalTabRow: false,
  tabGroups: [],
  tabOrder: [],
  savedTabGroups: [],
  hydrated: false,

  addRepo: (repo) => {
    set((s) => {
      const openRepos = s.openRepos.some((r) => r.id === repo.id || samePath(r.path, repo.path))
        ? s.openRepos
        : [...s.openRepos, repo]
      const represented = s.tabOrder.some((item) => item.type === 'repo' && samePath(item.path, repo.path))
        || groupForPath(s.tabGroups, repo.path) != null
      const tabOrder = represented ? s.tabOrder : [...s.tabOrder, { type: 'repo' as const, path: repo.path }]
      const recents = [
        { name: repo.name, path: repo.path },
        ...s.recents.filter((r) => r.path !== repo.path),
      ].slice(0, 10)
      return { openRepos, activeRepoId: repo.id, recents, tabOrder }
    })
    schedulePersist()
  },
  addReposInBackground: (repos) => {
    if (repos.length === 0) return
    set((s) => {
      let openRepos = s.openRepos
      let tabOrder = s.tabOrder
      let recents = s.recents
      for (const repo of repos) {
        if (openRepos.some((r) => r.id === repo.id || samePath(r.path, repo.path))) continue
        openRepos = [...openRepos, repo]
        const represented = tabOrder.some((item) => item.type === 'repo' && samePath(item.path, repo.path))
          || groupForPath(s.tabGroups, repo.path) != null
        if (!represented) tabOrder = [...tabOrder, { type: 'repo' as const, path: repo.path }]
        recents = [{ name: repo.name, path: repo.path }, ...recents.filter((r) => !samePath(r.path, repo.path))]
      }
      // activeRepoId is deliberately untouched: a batch open must not steal focus.
      // With no tab open yet there is nothing to steal, so focus the first one.
      const activeRepoId = s.activeRepoId ?? openRepos[0]?.id ?? null
      return { openRepos, activeRepoId, recents: recents.slice(0, 10), tabOrder }
    })
    schedulePersist()
  },
  removeRepo: (id) => {
    set((s) => {
      const removed = s.openRepos.find((repo) => repo.id === id)
      const openRepos = s.openRepos.filter((r) => r.id !== id)
      const workspace = removed
        ? removePathFromWorkspace(s.tabGroups, s.tabOrder, removed.path)
        : { groups: s.tabGroups, order: s.tabOrder }
      const orderedRemaining = workspace.order.flatMap((item) => {
        if (item.type === 'repo') return [item.path]
        return workspace.groups.find((group) => group.id === item.id)?.repoPaths ?? []
      })
      const nextActivePath = orderedRemaining.find((path) => openRepos.some((repo) => samePath(repo.path, path)))
      const activeRepoId =
        s.activeRepoId === id
          ? (openRepos.find((repo) => nextActivePath && samePath(repo.path, nextActivePath))?.id ?? openRepos[0]?.id ?? null)
          : s.activeRepoId
      return {
        openRepos,
        activeRepoId,
        tabGroups: workspace.groups,
        tabOrder: workspace.order,
      }
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
  setTagPushDefault: (mode) => {
    set({ tagPushDefault: mode })
    schedulePersist()
  },
  setTagPushOnCreate: (enabled) => {
    set({ tagPushOnCreate: enabled })
    schedulePersist()
  },
  setEnableWorktrees: (enabled) => {
    set({ enableWorktrees: enabled })
    schedulePersist()
  },
  setTabLayout: (layout) => {
    set({ tabLayout: layout })
    schedulePersist()
  },
  setHorizontalTabRow: (enabled) => {
    set({ horizontalTabRow: enabled })
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
  setShowRepoIcons: (enabled) => {
    set({ showRepoIcons: enabled })
    schedulePersist()
  },
  setTabIconOnly: (enabled) => {
    set({ tabIconOnly: enabled })
    schedulePersist()
  },
  setVerticalTabWidth: (width) => {
    set({ verticalTabWidth: clampVerticalTabWidth(width) })
    schedulePersist()
  },
  refreshRepoIcon: (path) => {
    const key = pathKey(path)
    set((s) => ({
      repoIconRevisions: {
        ...s.repoIconRevisions,
        [key]: (s.repoIconRevisions[key] ?? 0) + 1,
      },
    }))
  },
  createTabGroup: (repoPaths, options) => {
    const existingIds = new Set(get().tabGroups.map((group) => group.id))
    const requestedId = options?.id
    const id = requestedId && !existingIds.has(requestedId) ? requestedId : newGroupId()
    set((s) => {
      const paths = [...new Map(repoPaths
        .filter((path) => s.openRepos.some((repo) => samePath(repo.path, path)))
        .map((path) => [pathKey(path), normalizePath(path)])).values()]
      if (paths.length === 0) return s
      const positions = paths
        .map((path) => workspaceIndexForPath(s.tabGroups, s.tabOrder, path))
        .filter((index) => index >= 0)
      const insertionIndex = positions.length > 0 ? Math.min(...positions) : s.tabOrder.length
      let groups = s.tabGroups
      let order = s.tabOrder
      for (const path of paths) {
        const next = removePathFromWorkspace(groups, order, path)
        groups = next.groups
        order = next.order
      }
      const group: TabGroup = {
        id,
        name: options?.name?.trim() || 'New group',
        color: options?.color || TAB_GROUP_COLORS[groups.length % TAB_GROUP_COLORS.length],
        collapsed: false,
        repoPaths: paths,
      }
      groups = [...groups, group]
      order = [...order]
      order.splice(Math.min(insertionIndex, order.length), 0, { type: 'group', id })
      return { tabGroups: groups, tabOrder: order }
    })
    log.info(`workspace: created tab group ${id} with ${repoPaths.length} repo(s)`)
    schedulePersist()
    return id
  },
  addRepoToGroup: (repoPath, groupId) => {
    set((s) => {
      const currentGroup = groupForPath(s.tabGroups, repoPath)
      if (currentGroup?.id === groupId) return s
      const workspace = removePathFromWorkspace(s.tabGroups, s.tabOrder, repoPath)
      const target = workspace.groups.find((group) => group.id === groupId)
      if (!target) return s
      return {
        tabGroups: workspace.groups.map((group) => group.id === groupId
          ? { ...group, collapsed: false, repoPaths: [...group.repoPaths, normalizePath(repoPath)] }
          : group),
        tabOrder: workspace.order,
      }
    })
    log.info(`workspace: added repo to group ${groupId}`)
    schedulePersist()
  },
  removeRepoFromGroup: (repoPath) => {
    set((s) => {
      const group = groupForPath(s.tabGroups, repoPath)
      if (!group) return s
      const groupIndex = s.tabOrder.findIndex((item) => item.type === 'group' && item.id === group.id)
      const groupWillRemain = group.repoPaths.length > 1
      const workspace = removePathFromWorkspace(s.tabGroups, s.tabOrder, repoPath)
      const order = [...workspace.order]
      order.splice(Math.max(0, Math.min(groupIndex + (groupWillRemain ? 1 : 0), order.length)), 0, {
        type: 'repo',
        path: normalizePath(repoPath),
      })
      return { tabGroups: workspace.groups, tabOrder: order }
    })
    log.info('workspace: removed repo from its group')
    schedulePersist()
  },
  renameTabGroup: (groupId, name) => {
    const trimmed = name.trim()
    if (!trimmed) return
    set((s) => ({
      tabGroups: s.tabGroups.map((group) => group.id === groupId ? { ...group, name: trimmed } : group),
    }))
    schedulePersist()
  },
  setTabGroupColor: (groupId, color) => {
    set((s) => ({
      tabGroups: s.tabGroups.map((group) => group.id === groupId ? { ...group, color } : group),
    }))
    schedulePersist()
  },
  toggleTabGroup: (groupId) => {
    set((s) => ({
      tabGroups: s.tabGroups.map((group) => group.id === groupId
        ? { ...group, collapsed: !group.collapsed }
        : group),
    }))
    schedulePersist()
  },
  ungroupTabGroup: (groupId) => {
    set((s) => {
      const group = s.tabGroups.find((candidate) => candidate.id === groupId)
      const groupIndex = s.tabOrder.findIndex((item) => item.type === 'group' && item.id === groupId)
      if (!group || groupIndex < 0) return s
      const order = [...s.tabOrder]
      order.splice(groupIndex, 1, ...group.repoPaths.map((path) => ({ type: 'repo' as const, path })))
      return {
        tabGroups: s.tabGroups.filter((candidate) => candidate.id !== groupId),
        tabOrder: order,
      }
    })
    schedulePersist()
  },
  removeTabGroup: (groupId) => {
    set((s) => {
      const group = s.tabGroups.find((candidate) => candidate.id === groupId)
      if (!group) return s
      const closedKeys = new Set(group.repoPaths.map(pathKey))
      const openRepos = s.openRepos.filter((repo) => !closedKeys.has(pathKey(repo.path)))
      const activeRepoId = openRepos.some((repo) => repo.id === s.activeRepoId)
        ? s.activeRepoId
        : (openRepos[0]?.id ?? null)
      return {
        openRepos,
        activeRepoId,
        tabGroups: s.tabGroups.filter((candidate) => candidate.id !== groupId),
        tabOrder: s.tabOrder.filter((item) => !(item.type === 'group' && item.id === groupId)),
      }
    })
    schedulePersist()
  },
  saveTabGroup: (groupId) => {
    set((s) => {
      const group = s.tabGroups.find((candidate) => candidate.id === groupId)
      if (!group) return s
      const saved: SavedTabGroup = {
        id: group.id,
        name: group.name,
        color: group.color,
        repoPaths: [...group.repoPaths],
      }
      return {
        savedTabGroups: [...s.savedTabGroups.filter((candidate) => candidate.id !== groupId), saved],
      }
    })
    schedulePersist()
  },
  createSavedTabGroup: (repoPaths, name) => {
    const trimmedName = name.trim()
    const paths = [...new Map(repoPaths.map((path) => [pathKey(path), normalizePath(path)])).values()]
    if (!trimmedName || paths.length === 0) return null

    const id = newGroupId()
    set((s) => ({
      savedTabGroups: [...s.savedTabGroups, {
        id,
        name: trimmedName,
        color: TAB_GROUP_COLORS[s.savedTabGroups.length % TAB_GROUP_COLORS.length],
        repoPaths: paths,
      }],
    }))
    log.info(`workspace: saved new group ${id} with ${paths.length} repo(s)`)
    schedulePersist()
    return id
  },
  deleteSavedTabGroup: (groupId) => {
    set((s) => ({ savedTabGroups: s.savedTabGroups.filter((group) => group.id !== groupId) }))
    schedulePersist()
  },
  moveRepoBeside: (sourcePath, targetPath, placement) => {
    if (samePath(sourcePath, targetPath)) return
    set((s) => {
      const targetGroupId = groupForPath(s.tabGroups, targetPath)?.id ?? null
      const workspace = removePathFromWorkspace(s.tabGroups, s.tabOrder, sourcePath)
      if (targetGroupId) {
        const target = workspace.groups.find((group) => group.id === targetGroupId)
        if (!target) return s
        const targetIndex = target.repoPaths.findIndex((path) => samePath(path, targetPath))
        const groups = workspace.groups.map((group) => {
          if (group.id !== targetGroupId) return group
          const repoPaths = [...group.repoPaths]
          repoPaths.splice(targetIndex + (placement === 'after' ? 1 : 0), 0, normalizePath(sourcePath))
          return { ...group, collapsed: false, repoPaths }
        })
        return { tabGroups: groups, tabOrder: workspace.order }
      }
      const targetIndex = workspace.order.findIndex((item) => item.type === 'repo' && samePath(item.path, targetPath))
      if (targetIndex < 0) return s
      const order = [...workspace.order]
      order.splice(targetIndex + (placement === 'after' ? 1 : 0), 0, {
        type: 'repo',
        path: normalizePath(sourcePath),
      })
      return { tabGroups: workspace.groups, tabOrder: order }
    })
    schedulePersist()
  },
  moveRepoToOrder: (repoPath, requestedIndex) => {
    set((s) => {
      const sourceGroup = groupForPath(s.tabGroups, repoPath)
      const sourceIndex = workspaceIndexForPath(s.tabGroups, s.tabOrder, repoPath)
      const removesOrderItem = !sourceGroup || sourceGroup.repoPaths.length === 1
      let insertAt = requestedIndex
      if (removesOrderItem && sourceIndex >= 0 && sourceIndex < requestedIndex) insertAt -= 1
      const workspace = removePathFromWorkspace(s.tabGroups, s.tabOrder, repoPath)
      const order = [...workspace.order]
      order.splice(Math.max(0, Math.min(insertAt, order.length)), 0, {
        type: 'repo',
        path: normalizePath(repoPath),
      })
      return { tabGroups: workspace.groups, tabOrder: order }
    })
    schedulePersist()
  },
  moveGroupToOrder: (groupId, requestedIndex) => {
    set((s) => {
      const fromIndex = s.tabOrder.findIndex((item) => item.type === 'group' && item.id === groupId)
      if (fromIndex < 0) return s
      let insertAt = requestedIndex
      if (fromIndex < requestedIndex) insertAt -= 1
      const order = [...s.tabOrder]
      const [item] = order.splice(fromIndex, 1)
      if (!item) return s
      order.splice(Math.max(0, Math.min(insertAt, order.length)), 0, item)
      return { tabOrder: order }
    })
    schedulePersist()
  },
  finishRepoRestore: () => {
    set((s) => {
      const openKeys = new Set(s.openRepos.map((repo) => pathKey(repo.path)))
      const tabGroups = s.tabGroups
        .map((group) => ({ ...group, repoPaths: group.repoPaths.filter((path) => openKeys.has(pathKey(path))) }))
        .filter((group) => group.repoPaths.length > 0)
      const groupIds = new Set(tabGroups.map((group) => group.id))
      const represented = new Set(tabGroups.flatMap((group) => group.repoPaths.map(pathKey)))
      const tabOrder = s.tabOrder.filter((item) => {
        if (item.type === 'group') return groupIds.has(item.id)
        const key = pathKey(item.path)
        if (!openKeys.has(key) || represented.has(key)) return false
        represented.add(key)
        return true
      })
      for (const repo of s.openRepos) {
        const key = pathKey(repo.path)
        if (!represented.has(key)) {
          represented.add(key)
          tabOrder.push({ type: 'repo', path: repo.path })
        }
      }
      return { tabGroups, tabOrder }
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
    set({ columnOrder: DEFAULT_COLUMN_ORDER, hiddenColumns: [], columnWidths: {} })
    schedulePersist()
  },
  setColumnWidth: (id, width) => {
    set((s) => ({ columnWidths: { ...s.columnWidths, [id]: clampColumnWidth(id, width) } }))
    schedulePersist()
  },
  resetColumnWidth: (id) => {
    set((s) => {
      const columnWidths = { ...s.columnWidths }
      delete columnWidths[id]
      return { columnWidths }
    })
    schedulePersist()
  },
  setLeftPanelWidth: (width) => {
    set({ leftPanelWidth: clampLeftPanelWidth(width) })
    schedulePersist()
  },
  setRightPanelWidth: (width) => {
    set({ rightPanelWidth: clampRightPanelWidth(width) })
    schedulePersist()
  },
  setChangeSizeDisplay: (display) => {
    set({ changeSizeDisplay: display })
    schedulePersist()
  },
  setShowChangeIndicator: (enabled) => {
    set({ showChangeIndicator: enabled })
    schedulePersist()
  },
  setShowChangeLineCounts: (enabled) => {
    set({ showChangeLineCounts: enabled })
    schedulePersist()
  },

  hydrate: async () => {
    const settings = unwrap(await commands.getSettings())
    if (!get().hydrated) {
      const tabGroups = deserializeTabGroups(settings.tab_groups)
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
        columnWidths: normalizeColumnWidths(settings.column_layout?.widths),
        leftPanelWidth: clampLeftPanelWidth(settings.left_panel_width ?? DEFAULT_LEFT_PANEL_WIDTH),
        rightPanelWidth: clampRightPanelWidth(settings.right_panel_width ?? DEFAULT_RIGHT_PANEL_WIDTH),
        changeSizeDisplay: settings.change_size_display === 'row' ? 'row' : 'column',
        showChangeIndicator: settings.show_change_indicator ?? true,
        showChangeLineCounts: settings.show_change_line_counts ?? false,
        commitButtonMode: settings.commit_button_mode === 'commit_push' ? 'commit_push' : 'commit',
        tagPushDefault: normalizeTagPushDefault(settings.tag_push_default),
        tagPushOnCreate: settings.tag_push_on_create ?? false,
        enableWorktrees: settings.enable_worktrees ?? false,
        uiScale: settings.ui_scale != null ? clampUiScale(settings.ui_scale) : DEFAULT_UI_SCALE,
        tabAliases: normalizeAliases(settings.tab_aliases),
        showRepoIcons: settings.show_repo_icons ?? true,
        tabIconOnly: settings.tab_icon_only ?? false,
        verticalTabWidth: clampVerticalTabWidth(
          settings.vertical_tab_width ?? DEFAULT_VERTICAL_TAB_WIDTH,
        ),
        tabLayout: settings.tab_layout === 'vertical' ? 'vertical' : 'horizontal',
        horizontalTabRow: settings.horizontal_tab_row ?? false,
        tabGroups,
        tabOrder: deserializeTabOrder(settings.tab_order, tabGroups),
        savedTabGroups: deserializeSavedTabGroups(settings.saved_tab_groups),
        hydrated: true,
      })
    }
    return settings
  },
}))

// Commit selection, open diffs and conflicts all belong to one repo. Whenever the
// active repo changes - tab click, close, group close - drop them so the graph
// never queries the new repo for the old repo's objects.
useWorkspaceStore.subscribe((s, prev) => {
  if (s.activeRepoId !== prev.activeRepoId) useUiStore.getState().resetForRepoSwitch()
})

export function useActiveRepo(): RepoInfo | null {
  const openRepos = useWorkspaceStore((s) => s.openRepos)
  const activeRepoId = useWorkspaceStore((s) => s.activeRepoId)
  return openRepos.find((r) => r.id === activeRepoId) ?? null
}

// Dev-only: `openRepos` holds live repo handles that cannot be serialized, so a
// hot reload that re-evaluates this module would drop every open tab -- and the
// debounced persist would then write the empty list back to settings.json. Carry
// the state across reloads so editing a component never closes the user's repos.
// Production builds never run this.
if (import.meta.hot) {
  const carried = import.meta.hot.data.workspaceState as WorkspaceState | undefined
  if (carried) {
    useWorkspaceStore.setState(carried, true)
  }
  import.meta.hot.dispose((data) => {
    data.workspaceState = useWorkspaceStore.getState()
  })
}
