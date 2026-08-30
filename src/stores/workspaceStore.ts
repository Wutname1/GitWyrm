import { create } from "zustand";
import {
  commands,
  type BranchSwitchMode,
  type EditorKind,
  type MergeMethod,
  type RepoInfo,
  type Settings,
  type TelemetryLevel,
} from "@/lib/bindings";
import {
  addFolder,
  deserializeCodeFolders,
  primaryPathOf,
  removeFolder,
  setFolderLabel,
  setPrimaryFolder,
  type CodeFolder,
} from "@/lib/codeFolders";
import {
  applyDraft,
  clearDraft,
  type CommitDraft,
  type CommitDraftMap,
} from "@/lib/commitDrafts";
import { log } from "@/lib/log";
import { normalizePath, pathKey, samePath } from "@/lib/paths";
import { broadcastSettingsChanged } from "@/lib/settingsSync";
import { isTutorialRepoPath } from "@/lib/tutorialLessons";
import { unwrap } from "@/lib/queryKeys";
import {
  DEFAULT_COLUMN_ORDER,
  clampColumnWidth,
  normalizeColumnWidths,
  type ColumnId,
  type ColumnWidths,
} from "@/lib/graphColumns";
import { useUiStore } from "@/stores/uiStore";
import type { ThemeId, ThemeMode } from "@/lib/themes";
import { DEFAULT_FONT_ID } from "@/lib/fonts";

export interface RecentRepo {
  name: string;
  path: string;
}

export type UpdateChannel = "stable" | "beta";
export type CommitButtonMode = "commit" | "commit_push";
/** Ids of the editors the open-in-editor actions can launch. */
const EDITOR_KINDS = new Set<EditorKind>([
  "vs_code",
  "cursor",
  "windsurf",
  "jetbrains",
  "zed",
]);
export const DEFAULT_EDITOR: EditorKind = "vs_code";
export type ChangeSizeDisplay = "row" | "column";
/** How the changed-file lists are arranged: grouped by folder, or one flat row per file. */
export type ChangesViewMode = "tree" | "list";
/** Which column the repository picker's table is ordered by. */
export type RepoPickerSortKey = "name" | "activity" | "branch";
/** A column plus its direction, e.g. `activity` descending. */
export interface RepoPickerSort {
  key: RepoPickerSortKey;
  desc: boolean;
}
/** Open issue and pull request counts for one repository, from the last scan. */
export interface RepoActivity {
  prs: number;
  issues: number;
}

export type RepoPickerSection =
  | "pinned_groups"
  | "pinned_repositories"
  | "recent"
  | "watched";

const REPO_PICKER_SECTIONS = new Set<RepoPickerSection>([
  "pinned_groups",
  "pinned_repositories",
  "recent",
  "watched",
]);

/** What to do about local-only tags after a push. */
export type TagPushDefault = "ask" | "always" | "never";

/**
 * What to do with a worktree's branch once its folder is removed.
 *
 * "ask" shows the dialog every time. The other two are what "Remember my
 * choice" records, so the dialog stops appearing for the ordinary case.
 */
export type WorktreeBranchCleanup = "ask" | "keep" | "delete";

/**
 * Read a stored branch-cleanup choice, falling back to asking.
 *
 * The value is a plain string in settings.json, so anything unrecognised -- a
 * hand-edit, or a value written by a newer build -- must land on "ask". The
 * fallback is the one option that cannot delete anything by surprise.
 */
function readBranchCleanup(value: unknown): WorktreeBranchCleanup {
  return value === "keep" || value === "delete" ? value : "ask";
}

/**
 * A single repo's tag-setting overrides. Each field is optional: present means
 * this repo overrides that setting; absent means it follows the app-wide default.
 */
export interface TagOverride {
  pushDefault?: TagPushDefault;
  pushOnCreate?: boolean;
  deleteOnRemote?: boolean;
}

/** Tag settings resolved for one repo: app-wide default with any repo override applied. */
export interface ResolvedTagSettings {
  pushDefault: TagPushDefault;
  pushOnCreate: boolean;
  deleteOnRemote: boolean;
}
export type TabLayout = "horizontal" | "vertical";
export type TabDropPlacement = "before" | "after";

/**
 * How repository tabs are arranged. 'manual' is the drag-and-drop order the user
 * built; the other two are computed views that leave that order untouched, so
 * switching back to Manual restores exactly what they had.
 */
export type TabSort = "manual" | "name" | "changes" | "recent";

/**
 * Which way a sort runs. 'forward' is the natural reading of each rule -- A to Z
 * for name, busiest first for changes. Manual has no reverse: it is the order
 * the user dragged, so it ignores this entirely.
 */
export type TabSortDirection = "forward" | "reverse";

const TAB_SORTS = new Set<TabSort>(["manual", "name", "changes", "recent"]);

export function normalizeTabSort(sort: string | null | undefined): TabSort {
  return TAB_SORTS.has(sort as TabSort) ? (sort as TabSort) : "manual";
}

export function normalizeTabSortDirection(
  direction: string | null | undefined,
): TabSortDirection {
  return direction === "reverse" ? "reverse" : "forward";
}

export interface TabGroup {
  id: string;
  name: string;
  color: string;
  collapsed: boolean;
  repoPaths: string[];
  /**
   * Set only on a submodule group: the path of the project whose submodules
   * these are. The group is created for you when a submodule is opened, sits
   * directly under its parent tab, and its members cannot be dragged away --
   * a submodule belongs to its project, so letting one be filed somewhere else
   * would be a lie about where the folder lives.
   */
  parentPath?: string;
}

export interface SavedTabGroup {
  id: string;
  name: string;
  color: string;
  repoPaths: string[];
}

export type TabOrderItem =
  | { type: "repo"; path: string }
  | { type: "group"; id: string };

export const TAB_GROUP_COLORS = [
  "#2dd4a7",
  "#38bdf8",
  "#a78bfa",
  "#f59e0b",
  "#f472b6",
  "#f87171",
] as const;
export type { BranchSwitchMode };

/** Whole-app zoom limits and step, shared by the store and the status-bar control. */
export const MIN_UI_SCALE = 0.5;
export const MAX_UI_SCALE = 2.0;
export const UI_SCALE_STEP = 0.1;
export const DEFAULT_UI_SCALE = 1.0;

/**
 * Base UI text size in rem, before the whole-app zoom applies. The default
 * matches the original hard-coded body size (0.84375rem = 13.5px at a 16px root).
 */
export const MIN_FONT_SIZE = 0.6875;
export const MAX_FONT_SIZE = 1.125;
export const FONT_SIZE_STEP = 0.03125;
export const DEFAULT_FONT_SIZE = 0.875;

/** Base UI text weight. The default matches the original body weight. */
export const MIN_FONT_WEIGHT = 300;
export const MAX_FONT_WEIGHT = 600;
export const FONT_WEIGHT_STEP = 50;
export const DEFAULT_FONT_WEIGHT = 450;

/** Clamps and rounds a base font size (rem) into the supported range. */
export function clampFontSize(size: number): number {
  if (!Number.isFinite(size)) return DEFAULT_FONT_SIZE;
  const clamped = Math.min(MAX_FONT_SIZE, Math.max(MIN_FONT_SIZE, size));
  return Math.round(clamped / FONT_SIZE_STEP) * FONT_SIZE_STEP;
}

/** Clamps and snaps a font weight into the supported range. */
export function clampFontWeight(weight: number): number {
  if (!Number.isFinite(weight)) return DEFAULT_FONT_WEIGHT;
  const clamped = Math.min(MAX_FONT_WEIGHT, Math.max(MIN_FONT_WEIGHT, weight));
  return Math.round(clamped / FONT_WEIGHT_STEP) * FONT_WEIGHT_STEP;
}

/** Saved width limits for the vertical repository rail. */
export const MIN_VERTICAL_TAB_WIDTH = 48;
export const MAX_VERTICAL_TAB_WIDTH = 420;
export const DEFAULT_VERTICAL_TAB_WIDTH = 248;

/** Saved width limits for the main workspace panes. */
export const MIN_LEFT_PANEL_WIDTH = 176;
export const MAX_LEFT_PANEL_WIDTH = 420;
export const DEFAULT_LEFT_PANEL_WIDTH = 240;
export const MIN_RIGHT_PANEL_WIDTH = 240;
export const MAX_RIGHT_PANEL_WIDTH = 520;
export const DEFAULT_RIGHT_PANEL_WIDTH = 320;

/** Clamps a scale into the supported range and rounds to whole percent. */
export function clampUiScale(scale: number): number {
  if (!Number.isFinite(scale)) return DEFAULT_UI_SCALE;
  const clamped = Math.min(MAX_UI_SCALE, Math.max(MIN_UI_SCALE, scale));
  return Math.round(clamped * 100) / 100;
}

export function clampVerticalTabWidth(width: number): number {
  if (!Number.isFinite(width)) return DEFAULT_VERTICAL_TAB_WIDTH;
  return Math.round(
    Math.min(MAX_VERTICAL_TAB_WIDTH, Math.max(MIN_VERTICAL_TAB_WIDTH, width)),
  );
}

export function clampLeftPanelWidth(width: number): number {
  if (!Number.isFinite(width)) return DEFAULT_LEFT_PANEL_WIDTH;
  return Math.round(
    Math.min(MAX_LEFT_PANEL_WIDTH, Math.max(MIN_LEFT_PANEL_WIDTH, width)),
  );
}

export function clampRightPanelWidth(width: number): number {
  if (!Number.isFinite(width)) return DEFAULT_RIGHT_PANEL_WIDTH;
  return Math.round(
    Math.min(MAX_RIGHT_PANEL_WIDTH, Math.max(MIN_RIGHT_PANEL_WIDTH, width)),
  );
}

/** Saved size limits for the commit details drawer under the graph. */
export const MIN_DRAWER_HEIGHT = 120;
export const MAX_DRAWER_HEIGHT = 560;
export const DEFAULT_DRAWER_HEIGHT = 212;
export const MIN_DRAWER_LIST_WIDTH = 160;
export const MAX_DRAWER_LIST_WIDTH = 520;
export const DEFAULT_DRAWER_LIST_WIDTH = 280;

export function clampDrawerHeight(height: number): number {
  if (!Number.isFinite(height)) return DEFAULT_DRAWER_HEIGHT;
  return Math.round(
    Math.min(MAX_DRAWER_HEIGHT, Math.max(MIN_DRAWER_HEIGHT, height)),
  );
}

export function clampDrawerListWidth(width: number): number {
  if (!Number.isFinite(width)) return DEFAULT_DRAWER_LIST_WIDTH;
  return Math.round(
    Math.min(MAX_DRAWER_LIST_WIDTH, Math.max(MIN_DRAWER_LIST_WIDTH, width)),
  );
}

/**
 * Share of the changes pane given to the unstaged list, as a percentage of the
 * space the two lists share. Both ends are capped so neither list can shrink
 * past 30% and push the other off screen.
 */
export const MIN_CHANGES_SPLIT = 30;
export const MAX_CHANGES_SPLIT = 70;
export const DEFAULT_CHANGES_SPLIT = 50;

export function clampChangesSplit(split: number): number {
  if (!Number.isFinite(split)) return DEFAULT_CHANGES_SPLIT;
  return Math.round(
    Math.min(MAX_CHANGES_SPLIT, Math.max(MIN_CHANGES_SPLIT, split)),
  );
}

/**
 * Resting height of the commit description box, counted in lines of text. The
 * box always grows on its own up to five lines; this is how tall it sits when
 * empty, so someone who writes long descriptions can keep the room open.
 */
export const MIN_COMMIT_DESC_LINES = 2;
export const MAX_COMMIT_DESC_LINES = 12;
export const DEFAULT_COMMIT_DESC_LINES = 2;

export function clampCommitDescLines(lines: number): number {
  if (!Number.isFinite(lines)) return DEFAULT_COMMIT_DESC_LINES;
  return Math.round(
    Math.min(MAX_COMMIT_DESC_LINES, Math.max(MIN_COMMIT_DESC_LINES, lines)),
  );
}

/**
 * Share of the conflict view's width given to the OURS pane, as a percentage
 * of the space OURS and THEIRS share.
 */
export const MIN_CONFLICT_SIDE_SPLIT = 20;
export const MAX_CONFLICT_SIDE_SPLIT = 80;
export const DEFAULT_CONFLICT_SIDE_SPLIT = 50;

export function clampConflictSideSplit(split: number): number {
  if (!Number.isFinite(split)) return DEFAULT_CONFLICT_SIDE_SPLIT;
  return Math.round(
    Math.min(
      MAX_CONFLICT_SIDE_SPLIT,
      Math.max(MIN_CONFLICT_SIDE_SPLIT, split),
    ),
  );
}

/**
 * Share of the conflict view's height given to the OURS/THEIRS reference row,
 * as a percentage of the space it shares with the editable RESULT pane.
 */
export const MIN_CONFLICT_RESULT_SPLIT = 20;
export const MAX_CONFLICT_RESULT_SPLIT = 80;
export const DEFAULT_CONFLICT_RESULT_SPLIT = 45;

export function clampConflictResultSplit(split: number): number {
  if (!Number.isFinite(split)) return DEFAULT_CONFLICT_RESULT_SPLIT;
  return Math.round(
    Math.min(
      MAX_CONFLICT_RESULT_SPLIT,
      Math.max(MIN_CONFLICT_RESULT_SPLIT, split),
    ),
  );
}

/**
 * Key for one repo's staged or unstaged changes tree. Repo-scoped so two repos
 * with the same folder names keep their own open/closed state.
 */
export function changeTreeKey(repoPath: string, treeId: string): string {
  return `${pathKey(repoPath)}|${treeId}`;
}

function groupMarker(groupId: string): string {
  return `group:${groupId}`;
}

function groupForPath(groups: TabGroup[], path: string): TabGroup | undefined {
  return groups.find((group) =>
    group.repoPaths.some((candidate) => samePath(candidate, path)),
  );
}

/**
 * Move `sourcePath` beside `targetPath` within the pinned list. Returns the
 * list unchanged unless both paths are pinned, so dragging an unpinned tab next
 * to a pinned one never pulls it into the pinned strip.
 */
function reorderPinned(
  pinned: string[],
  sourcePath: string,
  targetPath: string,
  placeAfter: boolean,
): string[] {
  const isPinned = (path: string) =>
    pinned.some((candidate) => samePath(candidate, path));
  if (!isPinned(sourcePath) || !isPinned(targetPath)) return pinned;
  const rest = pinned.filter((candidate) => !samePath(candidate, sourcePath));
  const targetIndex = rest.findIndex((candidate) =>
    samePath(candidate, targetPath),
  );
  if (targetIndex < 0) return pinned;
  const next = [...rest];
  next.splice(targetIndex + (placeAfter ? 1 : 0), 0, normalizePath(sourcePath));
  return next;
}

function orderedRepoPaths(
  state: Pick<WorkspaceState, "tabGroups" | "tabOrder">,
): string[] {
  return state.tabOrder.flatMap((item) => {
    if (item.type === "repo") return [item.path];
    return (
      state.tabGroups.find((group) => group.id === item.id)?.repoPaths ?? []
    );
  });
}

function serializeTabOrder(order: TabOrderItem[]): string[] {
  return order.map((item) =>
    item.type === "group" ? groupMarker(item.id) : item.path,
  );
}

interface StoredTabGroup {
  id: string;
  name: string;
  color: string;
  collapsed?: boolean;
  repo_paths?: string[];
  parent_path?: string | null;
}

function deserializeTabGroups(
  groups: StoredTabGroup[] | undefined,
): TabGroup[] {
  const seen = new Set<string>();
  const result: TabGroup[] = [];
  for (const group of groups ?? []) {
    if (!group.id || seen.has(group.id)) continue;
    const repoPaths = [
      ...new Map(
        (group.repo_paths ?? []).map((path) => [
          pathKey(path),
          normalizePath(path),
        ]),
      ).values(),
    ];
    if (repoPaths.length === 0) continue;
    seen.add(group.id);
    result.push({
      id: group.id,
      name: group.name.trim() || "New group",
      color: group.color || TAB_GROUP_COLORS[0],
      collapsed: group.collapsed ?? false,
      repoPaths,
      ...(group.parent_path
        ? { parentPath: normalizePath(group.parent_path) }
        : {}),
    });
  }
  return result;
}

function deserializeSavedTabGroups(
  groups: StoredTabGroup[] | undefined,
): SavedTabGroup[] {
  return deserializeTabGroups(groups).map(({ id, name, color, repoPaths }) => ({
    id,
    name,
    color,
    repoPaths,
  }));
}

function deserializeTabOrder(
  order: string[] | undefined,
  groups: TabGroup[],
): TabOrderItem[] {
  const result: TabOrderItem[] = [];
  const seenGroups = new Set<string>();
  const seenRepos = new Set<string>();
  const groupedPaths = new Set(
    groups.flatMap((group) => group.repoPaths.map(pathKey)),
  );
  const groupIds = new Set(groups.map((group) => group.id));

  for (const value of order ?? []) {
    if (value.startsWith("group:")) {
      const id = value.slice("group:".length);
      if (groupIds.has(id) && !seenGroups.has(id)) {
        seenGroups.add(id);
        result.push({ type: "group", id });
      }
      continue;
    }
    const normalized = normalizePath(value);
    const key = pathKey(normalized);
    if (!groupedPaths.has(key) && !seenRepos.has(key)) {
      seenRepos.add(key);
      result.push({ type: "repo", path: normalized });
    }
  }
  for (const group of groups) {
    if (!seenGroups.has(group.id)) result.push({ type: "group", id: group.id });
  }
  return result;
}

function removePathFromWorkspace(
  groups: TabGroup[],
  order: TabOrderItem[],
  repoPath: string,
): { groups: TabGroup[]; order: TabOrderItem[] } {
  const trimmed = groups.map((group) => ({
    ...group,
    repoPaths: group.repoPaths.filter((path) => !samePath(path, repoPath)),
  }));
  // A submodule group holds its own project as its first member, so losing its
  // last submodule leaves it holding just that -- a group of one, wrapping a
  // tab that no longer has anything nested under it. Dissolve it back into an
  // ordinary tab, in the slot the group occupied.
  const dissolved = new Map<string, string>();
  const nextGroups = trimmed.filter((group) => {
    if (group.repoPaths.length === 0) return false;
    if (
      group.parentPath != null &&
      group.repoPaths.length === 1 &&
      samePath(group.repoPaths[0]!, group.parentPath)
    ) {
      dissolved.set(group.id, group.repoPaths[0]!);
      return false;
    }
    return true;
  });
  const survivingGroups = new Set(nextGroups.map((group) => group.id));
  const nextOrder = order.flatMap<TabOrderItem>((item) => {
    if (item.type === "repo")
      return samePath(item.path, repoPath) ? [] : [item];
    const loosened = dissolved.get(item.id);
    if (loosened != null) return [{ type: "repo", path: loosened }];
    return survivingGroups.has(item.id) ? [item] : [];
  });
  return { groups: nextGroups, order: nextOrder };
}

function workspaceIndexForPath(
  groups: TabGroup[],
  order: TabOrderItem[],
  repoPath: string,
): number {
  const group = groupForPath(groups, repoPath);
  return order.findIndex((item) =>
    group
      ? item.type === "group" && item.id === group.id
      : item.type === "repo" && samePath(item.path, repoPath),
  );
}

function newGroupId(): string {
  return `group-${Date.now()}-${Math.random().toString(36).slice(2, 7)}`;
}

/**
 * True for the auto-built group that holds a project's opened submodules. These
 * behave differently from groups the user made: they are named and placed for
 * the user, they cannot be dragged apart, and they disappear with their parent.
 */
export function isSubmoduleGroup(group: TabGroup): boolean {
  return group.parentPath != null;
}

/** The submodule group belonging to `parentPath`, if that project has one. */
function submoduleGroupFor(
  groups: TabGroup[],
  parentPath: string,
): TabGroup | undefined {
  return groups.find(
    (group) => group.parentPath != null && samePath(group.parentPath, parentPath),
  );
}

interface WorkspaceState {
  /** Repos currently open in tabs. */
  openRepos: RepoInfo[];
  /** Active repo id (tab). */
  activeRepoId: string | null;
  /**
   * Unfinished commit message per repo, keyed by repo id. Lets a draft survive
   * switching tabs and coming back.
   *
   * Deliberately in-memory only: this is working text, not a setting. A
   * half-written message that reappeared days after the app restarted would be
   * stale at best and misleading at worst. Cleared when the commit lands and
   * when the tab closes.
   */
  commitDrafts: CommitDraftMap;
  /** Recently opened repo paths, most recent first (persisted). */
  recents: RecentRepo[];
  /**
   * Folders scanned for quick-launch repos (persisted). Each is scanned on its
   * own, so an unplugged drive never holds up the others.
   */
  codeFolders: CodeFolder[];
  /**
   * Default directory new clones go into (persisted; falls back to the primary
   * code folder).
   */
  cloneDirectory: string | null;
  /** Path to the git executable for fetch/pull/push/clone. Empty lets GitWyrm pick: PATH, then its bundled copy (persisted). */
  gitExecutable: string;
  /** Path to the gpg executable used to sign commits. Empty lets GitWyrm pick: PATH, then its bundled copy (persisted). */
  gpgExecutable: string;
  /** Release channel used when checking for updates (persisted). */
  updateChannel: UpdateChannel;
  /** Install updates on the launch splash without asking (persisted). */
  autoUpdate: boolean;
  /**
   * Restart as soon as a download the user started finishes, instead of
   * waiting for them to press "Restart to update" (persisted).
   *
   * Separate from `autoUpdate`: that one covers the silent install at launch,
   * this one only removes the second click once the user has already chosen to
   * update.
   */
  autoRestartAfterDownload: boolean;
  /** What to do with uncommitted changes when switching branches (persisted). */
  branchSwitchMode: BranchSwitchMode;
  /**
   * How the last pull request was merged, reused as the default for the next
   * one (persisted).
   *
   * Deliberately has no Settings control: it is not a decision worth a screen,
   * it just remembers the choice made in the merge button's dropdown so someone
   * who always rebase-merges stops re-picking it. Left out of SETTINGS_GROUPS
   * for the same reason -- nothing visible to reset.
   */
  mergeMethod: MergeMethod;
  /**
   * The default AI provider: what every AI feature uses -- commit messages,
   * commit generation, and Spec Desk runs (persisted).
   *
   * Several providers can be configured at once (credentials are stored per
   * provider), so this names which one is in use rather than which one exists.
   * Read it through `resolveAiSelection` rather than directly, so no feature
   * decides for itself what "configured" means.
   */
  aiProvider: string | null;
  /** Model for the default provider. Mirrors `aiModels[aiProvider]` (persisted). */
  aiModel: string | null;
  /**
   * Model chosen per provider, keyed by provider id (persisted).
   *
   * Kept per provider so switching the default and switching back does not lose
   * the model each was set to -- with one global model, changing provider left a
   * model id that belonged to a different provider's catalog.
   */
  aiModels: Record<string, string>;
  /**
   * Whether AI features are switched on (persisted).
   *
   * Off is not the same as having no provider: it means "set up, but not right
   * now". Nothing about credentials changes, so turning it back on restores the
   * provider that was in use. Read it through `useAiSelection` rather than
   * directly -- every feature must see one answer.
   */
  aiEnabled: boolean;
  /** Custom commit-generation instruction; null uses the built-in default (persisted). */
  aiInstruction: string | null;
  /**
   * Commit message template for automatic spec-archive commits; null uses the
   * built-in default. `{id}` is replaced with the change id (persisted).
   */
  openspecArchiveCommitTemplate: string | null;
  /** Commit-graph column order (persisted). */
  columnOrder: ColumnId[];
  /** Commit-graph columns the user has hidden (persisted). */
  hiddenColumns: ColumnId[];
  /** Explicit commit-graph column widths (persisted). */
  columnWidths: ColumnWidths;
  /** Width of the branches and tags pane (persisted). */
  leftPanelWidth: number;
  /** Width of the changes and commit pane (persisted). */
  rightPanelWidth: number;
  /** Height of the commit details drawer under the graph (persisted). */
  drawerHeight: number;
  /** Width of the commit list pane inside the multi-select drawer (persisted). */
  drawerListWidth: number;
  /** Percent of the changes pane given to the unstaged list (persisted). */
  changesSplit: number;
  /** Resting height of the commit description box, in lines (persisted). */
  commitDescLines: number;
  /** Percent of the conflict view's width given to the OURS pane (persisted). */
  conflictSideSplit: number;
  /** Percent of the conflict view's height given to OURS/THEIRS (persisted). */
  conflictResultSplit: number;
  /** Where change size appears in the commit graph (persisted). */
  changeSizeDisplay: ChangeSizeDisplay;
  /** Whether commit rows show a change-size indicator (persisted). */
  showChangeIndicator: boolean;
  /** Whether the change-size indicator includes exact line counts (persisted). */
  showChangeLineCounts: boolean;
  /**
   * Whether commit graph nodes are drawn as the author's picture (persisted).
   *
   * The lane color does not go away when this is on: it becomes the ring around
   * the picture, so a branch is still identifiable at a glance.
   */
  showGraphAvatars: boolean;
  /** Default action for the commit button (persisted). */
  commitButtonMode: CommitButtonMode;
  /** Editor the open-in-editor actions launch (persisted). */
  defaultEditor: EditorKind;
  /** App-wide default for whether a push offers to send local-only tags (persisted). */
  tagPushDefault: TagPushDefault;
  /** App-wide default for whether the New Tag dialog's send box starts checked (persisted). */
  tagPushOnCreate: boolean;
  /**
   * App-wide default for whether the Delete Tag dialog's "also remove it from
   * the remote" box starts checked (persisted).
   */
  tagDeleteOnRemote: boolean;
  /**
   * Whether the "Discard all changes" dialog's "also reset submodules" box
   * starts checked. The box only appears when a submodule has actually moved
   * (persisted).
   */
  discardResetsSubmodules: boolean;
  /**
   * Per-repo tag settings that override the app-wide defaults, keyed by repo path.
   * A field is present only when that repo overrides it; absent fields follow the
   * app-wide default. A repo with no entry follows the defaults entirely (persisted).
   */
  tagOverridesByRepo: Record<string, TagOverride>;
  /**
   * Offer to *create* worktrees (persisted).
   *
   * Governs creation, not visibility: a worktree that already exists is always
   * listed and manageable, because a checkout the user cannot see is one they
   * cannot open, remove, or repair when it goes wrong.
   */
  enableWorktrees: boolean;
  /**
   * True once the user has deliberately set the worktrees toggle either way
   * (persisted).
   *
   * The auto-enable is a first-encounter courtesy. Without this, a deliberate
   * "off" is indistinguishable from "never asked" and the setting would keep
   * turning itself back on the next time a worktree appeared.
   */
  worktreesSettingTouched: boolean;
  /**
   * Show the Spec Desk button and the sidebar Specs section (persisted).
   *
   * On by default, and deliberately not tied to whether the repository has an
   * `openspec/` folder: a repo that has never used specs is exactly the one that
   * needs the way in. Off hides every Specs surface.
   */
  enableSpecDesk: boolean;
  /**
   * Archive a change from its row button without confirming first (persisted).
   *
   * Set only by the "Don't ask again" checkbox in that confirmation, and
   * resettable from the Spec Desk settings screen so the prompt can come back.
   */
  openspecArchiveWithoutAsking: boolean;
  /** Delete a change from its row button without confirming first (persisted). */
  openspecDeleteWithoutAsking: boolean;
  /**
   * What to do with a worktree's branch after removing the folder (persisted).
   *
   * "ask" is the default. A remembered "delete" still does not apply to a
   * branch with unmerged work: that case asks anyway, because the remembered
   * answer was given about a branch that had nothing to lose.
   */
  worktreeBranchCleanup: WorktreeBranchCleanup;
  /**
   * Whether that dialog's "delete it on the remote too" box starts checked
   * (persisted). Off by default -- deleting a published branch affects everyone
   * else working on it.
   */
  worktreeBranchDeleteOnRemote: boolean;
  /** Reopen the last session's tabs on launch. Off starts with no tabs (persisted). */
  restoreTabs: boolean;
  /** Fetch open repositories in the background to keep remote state current (persisted). */
  autoFetch: boolean;
  /**
   * Use the GitHub CLI when an organization blocks GitWyrm's own sign-in
   * (persisted). On by default; off means those repositories show nothing.
   */
  ghCliFallback: boolean;
  /**
   * Show the short explanations that teach a feature in the sidebar and panels
   * (persisted). Off hides the prose only: controls, counts, and the plain
   * empty-state labels stay, so no section can read as broken.
   */
  showTips: boolean;
  /**
   * How much GitWyrm reports about itself (persisted): `off`, `reports`
   * (errors and the install count), or `full` (those plus diagnostics).
   *
   * `null` means the user has never chosen, and the effective answer then
   * depends on the build -- `full` before 1.0 and on beta channels, `reports`
   * for stable releases. That rule lives in Rust; `telemetryLevelEffective`
   * below holds the answer it gave. Kept as a tri-state rather than collapsed
   * so a pre-1.0 install that never touched the setting narrows to `reports` at
   * 1.0 instead of freezing today's default into its settings file.
   *
   * Read at startup before the reporters initialize, so a change here takes
   * effect on the next launch rather than immediately.
   */
  telemetryLevel: TelemetryLevel | null;
  /**
   * What the telemetry level actually resolved to for this build, answered by
   * the backend at startup. This is what the control shows, so an un-chosen
   * install displays the state it is really in rather than nothing.
   */
  telemetryLevelEffective: TelemetryLevel;
  /**
   * Whether the welcome tour has run. App lifecycle state, not a preference:
   * deliberately absent from every reset group, since "reset my preferences"
   * should not replay onboarding (persisted).
   */
  onboardingSeen: boolean;
  /**
   * Fingerprints of signing keys the user confirmed they uploaded to their
   * host. A key is only half-set-up until its public half is published, and
   * that state has to outlive a reload (persisted).
   */
  signingKeysPublished: string[];
  /** Whole-app zoom factor, 1.0 = 100% (persisted). */
  uiScale: number;
  /** Selected UI font id; see lib/fonts.ts. 'plex' is the default (persisted). */
  fontFamily: string;
  /** Base UI text size in rem, before whole-app zoom (persisted). */
  fontSize: number;
  /** Base UI text weight (persisted). */
  fontWeight: number;
  /** Custom tab names, keyed by repo path. Missing paths use the repo folder name (persisted). */
  tabAliases: Record<string, string>;
  /** Selected color theme; 'auto' picks Slate (dark) or Paper (light) (persisted). */
  theme: ThemeId;
  /** Light/dark preference; 'system' follows the OS (persisted). */
  themeMode: ThemeMode;
  /** Use the mint accent across every theme; off reveals native accents (persisted). */
  mintAccent: boolean;
  /** Show favicon or logo images in repository tabs (persisted). */
  showRepoIcons: boolean;
  /** Hide repository names until a tab is hovered (persisted). */
  tabIconOnly: boolean;
  /** Width of the vertical repository rail in pixels (persisted). */
  verticalTabWidth: number;
  /** In-memory change counters used to refresh an icon everywhere it is shown. */
  repoIconRevisions: Record<string, number>;
  /** Whether repository tabs run across the top or down the left side (persisted). */
  tabLayout: TabLayout;
  /** Give horizontal tabs their own row under the app bar instead of sharing it (persisted). */
  horizontalTabRow: boolean;
  /** Show the open pull request count on repository tabs (persisted, off by default). */
  showTabPrCount: boolean;
  /** Show the open issue count on repository tabs (persisted, off by default). */
  showTabIssueCount: boolean;
  /** Groups that currently wrap open repository tabs (persisted while open). */
  tabGroups: TabGroup[];
  /** Shared order of loose repository tabs and complete groups (persisted). */
  tabOrder: TabOrderItem[];
  /** How tabs are arranged for display. 'manual' uses tabOrder as-is (persisted). */
  tabSort: TabSort;
  /** Which way the current sort runs. Ignored by 'manual' (persisted). */
  tabSortDirection: TabSortDirection;
  /**
   * When each tab was last switched to, as epoch milliseconds keyed by
   * lowercased repo path (persisted). Drives the 'recent' sort and the date
   * headings above it. Persisted because a tab strip that forgets every restart
   * would put every tab under one heading and the sort would say nothing.
   */
  tabLastUsedAt: Record<string, number>;
  /** Repo paths kept at the front of the tab strip, in pin order (persisted). */
  pinnedTabPaths: string[];
  /** Reusable group snapshots available from the repository picker (persisted). */
  savedTabGroups: SavedTabGroup[];
  /** Repository shortcuts shown first in the repository picker (persisted, newest pin first). */
  pinnedRepoPaths: string[];
  /** Saved-group shortcuts shown first in the repository picker (persisted, newest pin first). */
  pinnedSavedGroupIds: string[];
  /** Repository-picker sections the user has hidden (persisted). */
  repoPickerCollapsedSections: RepoPickerSection[];
  /** How the repository picker's table is ordered (persisted). */
  repoPickerSort: RepoPickerSort;
  /**
   * Open issue and pull request counts from the last scan, keyed by
   * `pathKey(path)`.
   *
   * Deliberately in-memory only. These are a live view of what is open on the
   * host, and a count restored from disk days later would be wrong in a way the
   * user cannot see -- better to show "never scanned" and let them press Scan.
   */
  repoActivity: Record<string, RepoActivity>;
  /** When the last scan finished, as epoch milliseconds. Null until one runs (persisted). */
  repoScanAt: number | null;
  /**
   * Folders left open in the changes trees, keyed by `changeTreeKey(repo, tree)`.
   * Only open folders are stored, so a folder the user never touched stays
   * collapsed and a deleted folder simply drops out on the next write (persisted).
   */
  expandedChangeFolders: Record<string, string[]>;
  /** Whether the changed-file lists group by folder or show one flat row per file (persisted). */
  changesViewMode: ChangesViewMode;
  /** True once settings.json has been read on launch. */
  hydrated: boolean;

  addRepo: (repo: RepoInfo) => void;
  /**
   * Opens a submodule as a tab attached to the project it came from. Builds the
   * parent's submodule group on first use and nests the tab inside it.
   */
  addSubmoduleRepo: (repo: RepoInfo, parentPath: string) => void;
  /** Opens several repos as tabs at once without changing which tab is active. */
  addReposInBackground: (repos: RepoInfo[]) => void;
  removeRepo: (id: string) => void;
  /**
   * Drop one entry from the recent list.
   *
   * Only the list is touched. A recent row is a bookmark, not the repo, so this
   * never deletes anything on disk - and it stays useful for the case it was
   * added for, where the folder is already gone and the row is the last thing
   * pointing at it.
   */
  removeRecent: (path: string) => void;
  setActiveRepo: (id: string) => void;
  /** Save the in-progress commit message for a repo. */
  setCommitDraft: (repoId: string, draft: Partial<CommitDraft>) => void;
  /** Forget a repo's draft, e.g. once the commit is written. */
  clearCommitDraft: (repoId: string) => void;
  /** Watch another folder. Ignored when already watched; the first becomes primary. */
  addCodeFolder: (path: string) => void;
  /** Stop watching a folder. Removing the primary promotes the next one. */
  removeCodeFolder: (path: string) => void;
  /** Make this the default clone destination and sort it first. */
  setPrimaryCodeFolder: (path: string) => void;
  /** Rename a folder for display. Blank clears back to showing the path. */
  setCodeFolderLabel: (path: string, label: string) => void;
  setCloneDirectory: (path: string | null) => void;
  /** Set the git executable path. Empty/blank falls back to PATH's git. */
  setGitExecutable: (path: string) => void;
  setGpgExecutable: (path: string) => void;
  setUpdateChannel: (channel: UpdateChannel) => void;
  setAutoUpdate: (enabled: boolean) => void;
  setAutoRestartAfterDownload: (enabled: boolean) => void;
  setBranchSwitchMode: (mode: BranchSwitchMode) => void;
  setMergeMethod: (method: MergeMethod) => void;
  setAiSelection: (provider: string | null, model: string | null) => void;
  /** Make a provider the default, restoring the model it was last set to. */
  setDefaultAiProvider: (provider: string | null) => void;
  /** Set one provider's model without making it the default. */
  setAiModelFor: (provider: string, model: string | null) => void;
  /** Switch AI features on or off, leaving every credential alone. */
  setAiEnabled: (enabled: boolean) => void;
  setAiInstruction: (instruction: string | null) => void;
  setOpenspecArchiveCommitTemplate: (template: string | null) => void;
  setCommitButtonMode: (mode: CommitButtonMode) => void;
  setDefaultEditor: (editor: EditorKind) => void;
  setTagPushDefault: (mode: TagPushDefault) => void;
  setTagPushOnCreate: (enabled: boolean) => void;
  setTagDeleteOnRemote: (enabled: boolean) => void;
  setDiscardResetsSubmodules: (enabled: boolean) => void;
  /**
   * Patch one repo's tag override. For each key, a value overrides that setting
   * for this repo; null clears it back to the app default. When no overridden
   * fields remain, the repo's entry is removed entirely.
   */
  setRepoTagOverride: (
    path: string,
    patch: {
      pushDefault?: TagPushDefault | null;
      pushOnCreate?: boolean | null;
      deleteOnRemote?: boolean | null;
    },
  ) => void;
  /** Remove a repo's override entirely so it follows the app-wide defaults. */
  clearRepoTagOverride: (path: string) => void;
  /** Resolve tag settings for a repo path: app-wide defaults with any override applied. */
  resolveTagSettings: (path: string | null | undefined) => ResolvedTagSettings;
  setEnableWorktrees: (enabled: boolean) => void;
  setEnableSpecDesk: (enabled: boolean) => void;
  setOpenspecArchiveWithoutAsking: (skip: boolean) => void;
  setOpenspecDeleteWithoutAsking: (skip: boolean) => void;
  setWorktreeBranchCleanup: (choice: WorktreeBranchCleanup) => void;
  setWorktreeBranchDeleteOnRemote: (on: boolean) => void;
  setRestoreTabs: (enabled: boolean) => void;
  setAutoFetch: (enabled: boolean) => void;
  setGhCliFallback: (enabled: boolean) => void;
  setShowTips: (enabled: boolean) => void;
  setTelemetryLevel: (level: TelemetryLevel) => void;
  markOnboardingSeen: () => void;
  markSigningKeyPublished: (fingerprint: string) => void;
  setTabLayout: (layout: TabLayout) => void;
  setHorizontalTabRow: (enabled: boolean) => void;
  setShowTabPrCount: (enabled: boolean) => void;
  setShowTabIssueCount: (enabled: boolean) => void;
  /** Set the whole-app zoom factor (clamped to the supported range). */
  setUiScale: (scale: number) => void;
  /** Set the UI font by id (see lib/fonts.ts). */
  setFontFamily: (id: string) => void;
  /** Set the base UI text size in rem (clamped to the supported range). */
  setFontSize: (size: number) => void;
  /** Set the base UI text weight (clamped to the supported range). */
  setFontWeight: (weight: number) => void;
  /** Rename a tab by repo path. An empty/blank alias clears it (back to the folder name). */
  setTabAlias: (path: string, alias: string) => void;
  setTheme: (theme: ThemeId) => void;
  setThemeMode: (mode: ThemeMode) => void;
  setMintAccent: (enabled: boolean) => void;
  setShowRepoIcons: (enabled: boolean) => void;
  setTabIconOnly: (enabled: boolean) => void;
  setVerticalTabWidth: (width: number) => void;
  refreshRepoIcon: (path: string) => void;
  createTabGroup: (
    repoPaths: string[],
    options?: { name?: string; color?: string; id?: string },
  ) => string;
  addRepoToGroup: (repoPath: string, groupId: string) => void;
  removeRepoFromGroup: (repoPath: string) => void;
  renameTabGroup: (groupId: string, name: string) => void;
  setTabGroupColor: (groupId: string, color: string) => void;
  toggleTabGroup: (groupId: string) => void;
  ungroupTabGroup: (groupId: string) => void;
  removeTabGroup: (groupId: string) => void;
  saveTabGroup: (groupId: string) => void;
  createSavedTabGroup: (repoPaths: string[], name: string) => string | null;
  deleteSavedTabGroup: (groupId: string) => void;
  togglePinnedRepo: (repoPath: string) => void;
  /**
   * Move a pinned repo so it sits directly before `targetPath` in the pinned
   * list. Dropping onto the last row appends instead, so a repo can be dragged
   * to the very bottom.
   */
  reorderPinnedRepo: (
    repoPath: string,
    targetPath: string,
    placeAfter?: boolean,
  ) => void;
  togglePinnedSavedGroup: (groupId: string) => void;
  toggleRepoPickerSection: (section: RepoPickerSection) => void;
  /** Sort by a column; picking the current column flips the direction. */
  sortRepoPicker: (key: RepoPickerSortKey) => void;
  /** Record the counts a scan came back with, and when it finished. */
  setRepoActivity: (activity: Record<string, RepoActivity>) => void;
  /**
   * Replace the open-folder set for one changes tree. An empty list drops the
   * entry entirely so settings.json does not accumulate keys for repos whose
   * folders are all collapsed.
   */
  setExpandedChangeFolders: (key: string, folders: string[]) => void;
  /** Switch the changed-file lists between the folder tree and the flat list. */
  setChangesViewMode: (mode: ChangesViewMode) => void;
  /**
   * Choose how tabs are arranged. Manual restores the user's dragged order.
   * Picking the sort that is already active flips its direction instead, so a
   * second click on Name turns A-Z into Z-A. Returns the direction now in
   * effect so the caller can say which way it went.
   */
  setTabSort: (sort: TabSort) => TabSortDirection;
  /**
   * Pin or unpin a tab. Pinned tabs always render before unpinned ones, in the
   * order they were pinned, whatever the sort is. Pinning a grouped repo pulls
   * it out of its group -- a pinned tab lives at the front, not inside a group.
   */
  toggleTabPin: (repoPath: string) => void;
  moveRepoBeside: (
    sourcePath: string,
    targetPath: string,
    placement: TabDropPlacement,
  ) => void;
  moveRepoToOrder: (repoPath: string, orderIndex: number) => void;
  /**
   * Adopt the arrangement currently on screen as the manual order and switch to
   * Manual. Dragging a tab while a sort rule is active means the user wants to
   * arrange things themselves, but writing straight to the stored order would
   * reflow the whole strip -- the sorted view and the manual order are usually
   * nothing alike. Freezing what they can see first makes the switch invisible,
   * so only the tab they dragged appears to move.
   */
  adoptDisplayedTabOrder: (displayed: TabOrderItem[]) => void;
  moveGroupToOrder: (groupId: string, orderIndex: number) => void;
  /** Removes paths that failed to reopen after launch. */
  finishRepoRestore: () => void;
  /** Move a column to a new index in the display order. */
  reorderColumn: (id: ColumnId, toIndex: number) => void;
  /** Show or hide a column. */
  toggleColumn: (id: ColumnId) => void;
  /**
   * Reset one settings screen's preferences to their defaults. Returns a
   * snapshot of the prior values so the UI can offer an undo. Does not touch
   * open repos, groups, or other runtime state.
   */
  resetSettingsGroup: (group: SettingsGroup) => SettingsSnapshot;
  /** Reset every preference to its default. Returns a snapshot for undo. */
  resetAllSettings: () => SettingsSnapshot;
  /** Re-apply a captured snapshot of settings values (used to undo a reset). */
  restoreSettings: (snapshot: SettingsSnapshot) => void;
  /** Restore the default column order and show every column. */
  resetColumns: () => void;
  /** Resize one graph column. */
  setColumnWidth: (id: ColumnId, width: number) => void;
  /** Return one graph column to its default sizing behavior. */
  resetColumnWidth: (id: ColumnId) => void;
  setLeftPanelWidth: (width: number) => void;
  setRightPanelWidth: (width: number) => void;
  setDrawerHeight: (height: number) => void;
  setDrawerListWidth: (width: number) => void;
  setChangesSplit: (split: number) => void;
  setCommitDescLines: (lines: number) => void;
  setConflictSideSplit: (split: number) => void;
  setConflictResultSplit: (split: number) => void;
  /** Put every draggable workspace panel back at its shipped size. */
  resetWorkspaceLayout: () => void;
  setChangeSizeDisplay: (display: ChangeSizeDisplay) => void;
  setShowChangeIndicator: (enabled: boolean) => void;
  setShowGraphAvatars: (enabled: boolean) => void;
  setShowChangeLineCounts: (enabled: boolean) => void;
  /** Reads settings.json once and hydrates the store; returns the raw settings for launch-time restore. */
  hydrate: () => Promise<Settings>;
  /**
   * Re-read the settings another window can change while this one is open.
   * Narrower than `hydrate`, which also owns tab and layout state.
   */
  reloadSettings: () => Promise<void>;
}

/** Fields persisted to settings.json (excludes in-memory-only state like openRepos handles). */
function toSettings(s: WorkspaceState): Settings {
  return {
    open_repos: orderedRepoPaths(s).filter((path) =>
      s.openRepos.some((repo) => samePath(repo.path, path)),
    ),
    // Normalized like open_repos above: this path comes straight off a RepoInfo,
    // which can carry the slash style the user typed ("C:/code/repo/"), and the
    // launch-time restore has to be able to match it against the reopened tabs.
    active_repo_path: (() => {
      const active = s.openRepos.find((r) => r.id === s.activeRepoId);
      return active ? normalizePath(active.path) : null;
    })(),
    recents: s.recents.map((r) => ({ name: r.name, path: r.path })),
    // Mirrored so a downgrade to a build that only knows one folder still finds
    // the one the user cares about most. code_folders is the source of truth.
    code_folder: primaryPathOf(s.codeFolders),
    code_folders: s.codeFolders.map((folder) => ({
      path: folder.path,
      label: folder.label,
      primary: folder.primary,
    })),
    clone_directory: s.cloneDirectory,
    git_executable: s.gitExecutable.trim() ? s.gitExecutable.trim() : null,
    gpg_executable: s.gpgExecutable.trim() ? s.gpgExecutable.trim() : null,
    update_channel: s.updateChannel === "beta" ? "beta" : "stable",
    auto_update: s.autoUpdate,
    auto_restart_after_download: s.autoRestartAfterDownload,
    branch_switch_mode: s.branchSwitchMode,
    merge_method: s.mergeMethod,
    ai_provider: s.aiProvider,
    // Still written so a downgrade keeps working; ai_models is the source of truth.
    ai_model: s.aiModel,
    ai_models: s.aiModels,
    ai_enabled: s.aiEnabled,
    ai_instruction: s.aiInstruction,
    openspec_archive_commit_template: s.openspecArchiveCommitTemplate,
    column_layout: {
      order: s.columnOrder,
      hidden: s.hiddenColumns,
      widths: s.columnWidths,
    },
    left_panel_width: s.leftPanelWidth,
    right_panel_width: s.rightPanelWidth,
    drawer_height: s.drawerHeight,
    drawer_commit_list_width: s.drawerListWidth,
    changes_split: s.changesSplit,
    commit_description_lines: s.commitDescLines,
    conflict_side_split: s.conflictSideSplit,
    conflict_result_split: s.conflictResultSplit,
    change_size_display: s.changeSizeDisplay,
    show_change_indicator: s.showChangeIndicator,
    show_change_line_counts: s.showChangeLineCounts,
    show_graph_avatars: s.showGraphAvatars,
    commit_button_mode: s.commitButtonMode,
    default_editor: s.defaultEditor,
    tag_push_default: s.tagPushDefault,
    tag_push_on_create: s.tagPushOnCreate,
    tag_delete_on_remote: s.tagDeleteOnRemote,
    discard_resets_submodules: s.discardResetsSubmodules,
    tag_overrides_by_repo: serializeTagOverrides(s.tagOverridesByRepo),
    enable_worktrees: s.enableWorktrees,
    worktrees_setting_touched: s.worktreesSettingTouched,
    enable_spec_desk: s.enableSpecDesk,
    openspec_archive_without_asking: s.openspecArchiveWithoutAsking,
    openspec_delete_without_asking: s.openspecDeleteWithoutAsking,
    worktree_branch_cleanup: s.worktreeBranchCleanup,
    worktree_branch_delete_on_remote: s.worktreeBranchDeleteOnRemote,
    restore_tabs: s.restoreTabs,
    show_tips: s.showTips,
    auto_fetch: s.autoFetch,
    gh_cli_fallback: s.ghCliFallback,
    telemetry_level: s.telemetryLevel,
    onboarding_seen: s.onboardingSeen,
    signing_keys_published: s.signingKeysPublished,
    ui_scale: s.uiScale,
    font_family: s.fontFamily === DEFAULT_FONT_ID ? null : s.fontFamily,
    font_size: s.fontSize,
    font_weight: s.fontWeight,
    tab_aliases: s.tabAliases,
    theme: s.theme === "auto" ? null : s.theme,
    theme_mode: s.themeMode === "system" ? null : s.themeMode,
    mint_accent: s.mintAccent,
    show_repo_icons: s.showRepoIcons,
    tab_icon_only: s.tabIconOnly,
    vertical_tab_width: s.verticalTabWidth,
    tab_layout: s.tabLayout,
    horizontal_tab_row: s.horizontalTabRow,
    show_tab_pr_count: s.showTabPrCount,
    show_tab_issue_count: s.showTabIssueCount,
    tab_groups: s.tabGroups.map((group) => ({
      id: group.id,
      name: group.name,
      color: group.color,
      collapsed: group.collapsed,
      repo_paths: group.repoPaths,
      parent_path: group.parentPath ?? null,
    })),
    tab_order: serializeTabOrder(s.tabOrder),
    tab_sort: s.tabSort,
    tab_sort_direction: s.tabSortDirection,
    tab_last_used_at: s.tabLastUsedAt,
    pinned_tab_paths: s.pinnedTabPaths,
    saved_tab_groups: s.savedTabGroups.map((group) => ({
      id: group.id,
      name: group.name,
      color: group.color,
      collapsed: false,
      repo_paths: group.repoPaths,
    })),
    pinned_repo_paths: s.pinnedRepoPaths,
    pinned_saved_group_ids: s.pinnedSavedGroupIds,
    repo_picker_collapsed_sections: s.repoPickerCollapsedSections,
    repo_picker_sort: `${s.repoPickerSort.key}${s.repoPickerSort.desc ? ":desc" : ""}`,
    repo_scan_at: s.repoScanAt == null ? null : new Date(s.repoScanAt).toISOString(),
    expanded_change_folders: s.expandedChangeFolders,
    changes_view_mode: s.changesViewMode,
  };
}

/** Column ids known to this build; anything else in persisted layout is dropped. */
const KNOWN_COLUMNS = new Set<ColumnId>(DEFAULT_COLUMN_ORDER);

/**
 * Builds the per-provider model map from settings.
 *
 * Two jobs. It drops empty entries -- the persisted map's values are optional,
 * and a provider mapped to nothing is worse than a provider mapped to nothing at
 * all. And it seeds the map from the older single `ai_provider`/`ai_model` pair,
 * so an install from before per-provider models keeps its choice without the user
 * reconfiguring. A stored map entry wins over the seed where both exist.
 */
function normalizeAiModels(
  stored: Partial<Record<string, string>> | null | undefined,
  legacyProvider: string | null | undefined,
  legacyModel: string | null | undefined,
): Record<string, string> {
  const result: Record<string, string> = {};
  if (legacyProvider && legacyModel) {
    result[legacyProvider] = legacyModel;
  }
  for (const [provider, model] of Object.entries(stored ?? {})) {
    if (model) {
      result[provider] = model;
    }
  }
  return result;
}

/**
 * Sanitizes a persisted order: keeps only known ids, drops duplicates, and
 * appends any columns missing from the saved order (e.g. added in a new build)
 * so every column stays reachable.
 */
function normalizeOrder(order: string[] | undefined): ColumnId[] {
  const seen = new Set<ColumnId>();
  const result: ColumnId[] = [];
  for (const id of order ?? []) {
    if (isColumnId(id) && !seen.has(id)) {
      seen.add(id);
      result.push(id);
    }
  }
  // Older settings have no Changes column. Place it beside Author, matching
  // the new default, while preserving every other saved column position.
  if (!seen.has("changes")) {
    const authorIndex = result.indexOf("author");
    if (authorIndex >= 0) {
      result.splice(authorIndex + 1, 0, "changes");
      seen.add("changes");
    }
  }
  for (const id of DEFAULT_COLUMN_ORDER) {
    if (!seen.has(id)) result.push(id);
  }
  return result;
}

function normalizeHidden(hidden: string[] | undefined): ColumnId[] {
  return (hidden ?? []).filter(isColumnId);
}

function isColumnId(id: string): id is ColumnId {
  return KNOWN_COLUMNS.has(id as ColumnId);
}

/** Unknown or missing values fall back to asking, the safest of the three. */
function normalizeTagPushDefault(
  mode: string | null | undefined,
): TagPushDefault {
  return mode === "always" || mode === "never" ? mode : "ask";
}

function normalizeRepoPickerSections(
  sections: string[] | undefined,
): RepoPickerSection[] {
  return (sections ?? []).filter((section): section is RepoPickerSection =>
    REPO_PICKER_SECTIONS.has(section as RepoPickerSection),
  );
}

/** Validate a stored sort choice; anything unrecognised falls back to name ascending. */
function normalizeRepoPickerSort(
  sort: string | null | undefined,
): RepoPickerSort {
  const [key, direction] = (sort ?? "").split(":");
  if (key !== "name" && key !== "activity" && key !== "branch") {
    return { key: "name", desc: false };
  }
  return { key, desc: direction === "desc" };
}

/** Validate a stored scan timestamp; an unparseable one reads as never scanned. */
function normalizeScanAt(at: string | null | undefined): number | null {
  if (!at) return null;
  const parsed = Date.parse(at);
  return Number.isNaN(parsed) ? null : parsed;
}

/** Validate a stored theme id; unknown/absent falls back to Auto. */
function normalizeTheme(theme: string | null | undefined): ThemeId {
  return theme === "slate" ||
    theme === "onyx" ||
    theme === "midnight" ||
    theme === "paper"
    ? theme
    : "auto";
}

/** Validate a stored theme mode; unknown/absent falls back to system. */
function normalizeThemeMode(mode: string | null | undefined): ThemeMode {
  return mode === "light" || mode === "dark" ? mode : "system";
}

/**
 * Settings map values arrive as `string | undefined` (a Rust HashMap maps to a
 * Partial record). Drop the empty entries so the store holds a plain map.
 */
function normalizeAliases(
  aliases: Partial<Record<string, string>> | undefined,
): Record<string, string> {
  const out: Record<string, string> = {};
  for (const [path, alias] of Object.entries(aliases ?? {})) {
    if (alias) out[normalizePath(path)] = alias;
  }
  return out;
}

/**
 * Read the saved open-folder map back, dropping empty entries. Folder paths are
 * not validated against the working tree: a folder that no longer has changes is
 * simply never rendered, and drops out the next time the tree is written.
 */
function normalizeExpandedChangeFolders(
  folders: Partial<Record<string, string[]>> | undefined,
): Record<string, string[]> {
  const out: Record<string, string[]> = {};
  for (const [key, value] of Object.entries(folders ?? {})) {
    if (value && value.length > 0) out[key] = value;
  }
  return out;
}

/**
 * Validate stored last-used stamps. Keys are re-normalized because a path
 * saved on an earlier run may not match how we key paths now, and non-finite
 * or negative stamps are dropped rather than sorted as if they were real -- a
 * junk timestamp would strand a tab under the wrong heading forever.
 */
function normalizeTabLastUsedAt(
  stamps: Partial<Record<string, number>> | undefined,
): Record<string, number> {
  const out: Record<string, number> = {};
  for (const [key, value] of Object.entries(stamps ?? {})) {
    if (typeof value !== "number" || !Number.isFinite(value) || value <= 0)
      continue;
    out[pathKey(key)] = value;
  }
  return out;
}

/** Wire shape of a per-repo tag override as stored in settings.json (snake_case, nullable fields). */
type StoredTagOverride = {
  push_default?: string | null;
  push_on_create?: boolean | null;
  delete_on_remote?: boolean | null;
};

/** Serialize the override map for persistence. Empty overrides are dropped. */
function serializeTagOverrides(
  overrides: Record<string, TagOverride>,
): Record<string, StoredTagOverride> {
  const out: Record<string, StoredTagOverride> = {};
  for (const [path, value] of Object.entries(overrides)) {
    const stored: StoredTagOverride = {};
    if (value.pushDefault !== undefined)
      stored.push_default = value.pushDefault;
    if (value.pushOnCreate !== undefined)
      stored.push_on_create = value.pushOnCreate;
    if (value.deleteOnRemote !== undefined)
      stored.delete_on_remote = value.deleteOnRemote;
    if (
      stored.push_default !== undefined ||
      stored.push_on_create !== undefined ||
      stored.delete_on_remote !== undefined
    ) {
      out[path] = stored;
    }
  }
  return out;
}

/** Read the override map back from settings, dropping empty or invalid entries. */
function normalizeTagOverrides(
  overrides: Partial<Record<string, StoredTagOverride>> | undefined,
): Record<string, TagOverride> {
  const out: Record<string, TagOverride> = {};
  for (const [path, stored] of Object.entries(overrides ?? {})) {
    if (!stored) continue;
    const value: TagOverride = {};
    if (
      stored.push_default === "ask" ||
      stored.push_default === "always" ||
      stored.push_default === "never"
    ) {
      value.pushDefault = stored.push_default;
    }
    if (typeof stored.push_on_create === "boolean") {
      value.pushOnCreate = stored.push_on_create;
    }
    if (typeof stored.delete_on_remote === "boolean") {
      value.deleteOnRemote = stored.delete_on_remote;
    }
    if (
      value.pushDefault !== undefined ||
      value.pushOnCreate !== undefined ||
      value.deleteOnRemote !== undefined
    ) {
      out[path] = value;
    }
  }
  return out;
}

let persistTimer: ReturnType<typeof setTimeout> | null = null;

/** Debounced write-through to settings.json; skipped until hydration completes. */
function schedulePersist() {
  const s = useWorkspaceStore.getState();
  if (!s.hydrated) return;
  if (persistTimer) clearTimeout(persistTimer);
  persistTimer = setTimeout(() => {
    persistTimer = null;
    // Broadcast only after the write resolves: a window that re-reads before
    // the file lands would pick up the previous values and overwrite its own
    // correct state with them.
    void commands
      .saveSettings(toSettings(useWorkspaceStore.getState()))
      .then(broadcastSettingsChanged);
  }, 300);
}

/**
 * Write any pending settings immediately. Called as the window closes: switching
 * tabs and quitting straight away is a normal thing to do, and without this the
 * debounced write never fires, so the next launch reopens the tab before last.
 */
export function flushPendingSettings(): Promise<unknown> {
  if (!useWorkspaceStore.getState().hydrated) return Promise.resolve();
  if (persistTimer) {
    clearTimeout(persistTimer);
    persistTimer = null;
  }
  return commands
    .saveSettings(toSettings(useWorkspaceStore.getState()))
    .then((r) => {
      broadcastSettingsChanged();
      return r;
    });
}

/**
 * Default values for every user-facing preference, the single source of truth
 * for the per-screen and global "Reset to defaults" actions. Runtime state
 * (open repos, hydration, icon revisions, tab layout data) is intentionally
 * excluded: resetting preferences must never close tabs or drop groups.
 */
export const SETTINGS_DEFAULTS = {
  codeFolders: [],
  cloneDirectory: null,
  repoPickerCollapsedSections: [],
  repoPickerSort: { key: "name", desc: false },
  gitExecutable: "",
  gpgExecutable: "",
  // Not in any per-screen group: only the global "Reset all" restores them.
  updateChannel: "stable",
  autoUpdate: true,
  autoRestartAfterDownload: false,
  branchSwitchMode: "auto_stash",
  commitButtonMode: "commit",
  defaultEditor: DEFAULT_EDITOR,
  enableWorktrees: false,
  worktreesSettingTouched: false,
  enableSpecDesk: true,
  openspecArchiveWithoutAsking: false,
  openspecDeleteWithoutAsking: false,
  worktreeBranchCleanup: "ask",
  worktreeBranchDeleteOnRemote: false,
  restoreTabs: true,
  autoFetch: true,
  ghCliFallback: true,
  showTips: true,
  // Deliberately outside the per-screen "behavior" group below: a privacy
  // choice must not change because the user reset an unrelated page of
  // preferences. Only the global "Reset all" restores it.
  //
  // null is the real default: no choice recorded, so the build decides. A
  // "Reset all" puts it back to un-chosen rather than to a fixed level.
  telemetryLevel: null,
  telemetryLevelEffective: "full",
  onboardingSeen: false,
  signingKeysPublished: [],
  tagPushDefault: "ask",
  tagPushOnCreate: true,
  tagDeleteOnRemote: true,
  discardResetsSubmodules: true,
  aiInstruction: null,
  openspecArchiveCommitTemplate: null,
  changeSizeDisplay: "column",
  showChangeIndicator: false,
  showChangeLineCounts: false,
  showGraphAvatars: false,
  uiScale: DEFAULT_UI_SCALE,
  fontFamily: DEFAULT_FONT_ID,
  fontSize: DEFAULT_FONT_SIZE,
  fontWeight: DEFAULT_FONT_WEIGHT,
  theme: "auto",
  themeMode: "system",
  mintAccent: true,
  showRepoIcons: true,
  tabIconOnly: false,
  tabLayout: "vertical",
  horizontalTabRow: false,
  leftPanelWidth: DEFAULT_LEFT_PANEL_WIDTH,
  rightPanelWidth: DEFAULT_RIGHT_PANEL_WIDTH,
  drawerHeight: DEFAULT_DRAWER_HEIGHT,
  drawerListWidth: DEFAULT_DRAWER_LIST_WIDTH,
  changesSplit: DEFAULT_CHANGES_SPLIT,
  commitDescLines: DEFAULT_COMMIT_DESC_LINES,
  conflictSideSplit: DEFAULT_CONFLICT_SIDE_SPLIT,
  conflictResultSplit: DEFAULT_CONFLICT_RESULT_SPLIT,
  verticalTabWidth: DEFAULT_VERTICAL_TAB_WIDTH,
  changesViewMode: "tree",
  showTabPrCount: true,
  showTabIssueCount: true,
} satisfies Partial<WorkspaceState>;

/** A resettable preference key. */
export type SettingsKey = keyof typeof SETTINGS_DEFAULTS;

/** Which preference keys each settings screen owns, for per-screen reset. */
export const SETTINGS_GROUPS = {
  general: [
    "codeFolders",
    "cloneDirectory",
    "branchSwitchMode",
    "commitButtonMode",
    "defaultEditor",
    "enableWorktrees",
    "worktreesSettingTouched",
    // Resetting General brings the branch-cleanup prompt back, which is the
    // only way to undo a "Remember my choice" the user later regrets.
    "worktreeBranchCleanup",
    "worktreeBranchDeleteOnRemote",
    "tabLayout",
    "horizontalTabRow",
  ],
  behavior: [
    "restoreTabs",
    "autoFetch",
    "ghCliFallback",
    "showTips",
    "discardResetsSubmodules",
  ],
  tags: ["tagPushDefault", "tagPushOnCreate", "tagDeleteOnRemote"],
  ai: ["aiInstruction"],
  openspec: [
    "enableSpecDesk",
    "openspecArchiveCommitTemplate",
    // Resetting this screen puts both confirmations back, which is the only way
    // to undo a "Don't ask again" the user later regrets.
    "openspecArchiveWithoutAsking",
    "openspecDeleteWithoutAsking",
  ],
  appearance: [
    "changeSizeDisplay",
    "showChangeIndicator",
    "showChangeLineCounts",
    "showGraphAvatars",
    "uiScale",
    "fontFamily",
    "fontSize",
    "fontWeight",
    "theme",
    "themeMode",
    "mintAccent",
    "showRepoIcons",
    "tabIconOnly",
    "changesViewMode",
  ],
  // Only the tab badges. Resetting this screen must not sign the user out of
  // GitHub -- disconnecting is a deliberate act with its own button.
  integrations: ["showTabPrCount", "showTabIssueCount"],
} satisfies Record<string, SettingsKey[]>;

export type SettingsGroup = keyof typeof SETTINGS_GROUPS;

/** A captured slice of settings values, used to undo a reset. */
export type SettingsSnapshot = Partial<Pick<WorkspaceState, SettingsKey>>;

export const useWorkspaceStore = create<WorkspaceState>()((set, get) => ({
  openRepos: [],
  activeRepoId: null,
  commitDrafts: {},
  recents: [],
  codeFolders: [],
  cloneDirectory: null,
  gitExecutable: "",
  gpgExecutable: "",
  updateChannel: "stable",
  autoUpdate: true,
  autoRestartAfterDownload: false,
  branchSwitchMode: "auto_stash",
  mergeMethod: "merge",
  aiProvider: null,
  aiModel: null,
  aiModels: {},
  aiEnabled: true,
  aiInstruction: null,
  openspecArchiveCommitTemplate: null,
  columnOrder: DEFAULT_COLUMN_ORDER,
  hiddenColumns: [],
  columnWidths: {},
  leftPanelWidth: DEFAULT_LEFT_PANEL_WIDTH,
  rightPanelWidth: DEFAULT_RIGHT_PANEL_WIDTH,
  drawerHeight: DEFAULT_DRAWER_HEIGHT,
  drawerListWidth: DEFAULT_DRAWER_LIST_WIDTH,
  changesSplit: DEFAULT_CHANGES_SPLIT,
  commitDescLines: DEFAULT_COMMIT_DESC_LINES,
  conflictSideSplit: DEFAULT_CONFLICT_SIDE_SPLIT,
  conflictResultSplit: DEFAULT_CONFLICT_RESULT_SPLIT,
  changeSizeDisplay: "column",
  showChangeIndicator: false,
  showChangeLineCounts: false,
  showGraphAvatars: false,
  commitButtonMode: "commit",
  defaultEditor: DEFAULT_EDITOR,
  tagPushDefault: "ask",
  tagPushOnCreate: true,
  tagDeleteOnRemote: true,
  discardResetsSubmodules: true,
  tagOverridesByRepo: {},
  enableWorktrees: false,
  worktreesSettingTouched: false,
  enableSpecDesk: true,
  openspecArchiveWithoutAsking: false,
  openspecDeleteWithoutAsking: false,
  worktreeBranchCleanup: "ask",
  worktreeBranchDeleteOnRemote: false,
  restoreTabs: true,
  autoFetch: true,
  ghCliFallback: true,
  showTips: true,
  telemetryLevel: null,
  // Optimistic until the backend answers at startup; corrected by hydration
  // before the settings screen can be opened.
  telemetryLevelEffective: "full",
  onboardingSeen: false,
  signingKeysPublished: [],
  uiScale: DEFAULT_UI_SCALE,
  fontFamily: DEFAULT_FONT_ID,
  fontSize: DEFAULT_FONT_SIZE,
  fontWeight: DEFAULT_FONT_WEIGHT,
  tabAliases: {},
  theme: "auto",
  themeMode: "system",
  mintAccent: true,
  showRepoIcons: true,
  tabIconOnly: false,
  verticalTabWidth: DEFAULT_VERTICAL_TAB_WIDTH,
  repoIconRevisions: {},
  tabLayout: "vertical",
  horizontalTabRow: false,
  showTabPrCount: true,
  showTabIssueCount: true,
  tabGroups: [],
  tabOrder: [],
  tabSort: "manual",
  tabSortDirection: "forward",
  tabLastUsedAt: {},
  pinnedTabPaths: [],
  savedTabGroups: [],
  pinnedRepoPaths: [],
  pinnedSavedGroupIds: [],
  repoPickerCollapsedSections: [],
  repoPickerSort: { key: "name", desc: false },
  repoActivity: {},
  repoScanAt: null,
  expandedChangeFolders: {},
  changesViewMode: "tree",
  hydrated: false,

  addRepo: (repo) => {
    set((s) => {
      const openRepos = s.openRepos.some(
        (r) => r.id === repo.id || samePath(r.path, repo.path),
      )
        ? s.openRepos
        : [...s.openRepos, repo];
      const represented =
        s.tabOrder.some(
          (item) => item.type === "repo" && samePath(item.path, repo.path),
        ) || groupForPath(s.tabGroups, repo.path) != null;
      const tabOrder = represented
        ? s.tabOrder
        : [...s.tabOrder, { type: "repo" as const, path: repo.path }];
      // The tutorial's practice repo is deleted when the tour ends, so putting
      // it in recents would leave a row pointing at a folder that is gone.
      const recents = isTutorialRepoPath(repo.path)
        ? s.recents
        : [
            { name: repo.name, path: repo.path },
            ...s.recents.filter((r) => !samePath(r.path, repo.path)),
          ].slice(0, 10);
      return {
        openRepos,
        activeRepoId: repo.id,
        recents,
        tabOrder,
        // Opening a repo selects it, and this path skips setActiveRepo, so
        // stamp it here or a freshly opened tab would sort as never-used.
        tabLastUsedAt: { ...s.tabLastUsedAt, [pathKey(repo.path)]: Date.now() },
      };
    });
    schedulePersist();
  },
  addSubmoduleRepo: (repo, parentPath) => {
    set((s) => {
      const parent = s.openRepos.find((candidate) =>
        samePath(candidate.path, parentPath),
      );
      // The parent tab is what a submodule group hangs from. With it gone there
      // is nothing to attach to, so the submodule opens as an ordinary tab.
      if (!parent) return s;

      const path = normalizePath(repo.path);
      const openRepos = s.openRepos.some(
        (r) => r.id === repo.id || samePath(r.path, repo.path),
      )
        ? s.openRepos
        : [...s.openRepos, repo];

      // Wherever this path sat before -- loose, or in some other group -- it now
      // belongs to its parent, so clear it out before filing it.
      const cleared = removePathFromWorkspace(s.tabGroups, s.tabOrder, path);
      let groups = cleared.groups;
      let order = cleared.order;

      const parentKey = normalizePath(parent.path);
      const existing = submoduleGroupFor(groups, parent.path);
      const groupId = existing?.id ?? newGroupId();
      if (existing) {
        groups = groups.map((group) =>
          group.id === groupId
            ? { ...group, collapsed: false, repoPaths: [...group.repoPaths, path] }
            : group,
        );
      } else {
        // The project's own tab is the first member, not a separate row above
        // the group: the submodules attach directly under the tab they belong
        // to. Taking the parent out of the loose order is what stops it being
        // drawn twice -- and the group then takes the slot the parent held, so
        // the project does not jump to the end of the strip.
        const parentIndex = order.findIndex(
          (item) => item.type === "repo" && samePath(item.path, parentKey),
        );
        const lifted = removePathFromWorkspace(groups, order, parentKey);
        groups = [
          ...lifted.groups,
          {
            id: groupId,
            name: parent.name,
            // The parent's own accent, so the nested block reads as part of it
            // rather than as another colour-coded group the user picked.
            color: TAB_GROUP_COLORS[groups.length % TAB_GROUP_COLORS.length]!,
            collapsed: false,
            repoPaths: [parentKey, path],
            parentPath: parentKey,
          },
        ];
        order = [...lifted.order];
        order.splice(parentIndex >= 0 ? parentIndex : order.length, 0, {
          type: "group",
          id: groupId,
        });
      }

      const recents = isTutorialRepoPath(repo.path)
        ? s.recents
        : [
            { name: repo.name, path: repo.path },
            ...s.recents.filter((r) => !samePath(r.path, repo.path)),
          ].slice(0, 10);
      return {
        openRepos,
        activeRepoId: repo.id,
        recents,
        tabGroups: groups,
        tabOrder: order,
      };
    });
    log.info("workspace: opened a submodule attached to its project");
    schedulePersist();
  },
  addReposInBackground: (repos) => {
    if (repos.length === 0) return;
    set((s) => {
      let openRepos = s.openRepos;
      let tabOrder = s.tabOrder;
      let recents = s.recents;
      for (const repo of repos) {
        if (
          openRepos.some((r) => r.id === repo.id || samePath(r.path, repo.path))
        )
          continue;
        openRepos = [...openRepos, repo];
        const represented =
          tabOrder.some(
            (item) => item.type === "repo" && samePath(item.path, repo.path),
          ) || groupForPath(s.tabGroups, repo.path) != null;
        if (!represented)
          tabOrder = [...tabOrder, { type: "repo" as const, path: repo.path }];
        recents = [
          { name: repo.name, path: repo.path },
          ...recents.filter((r) => !samePath(r.path, repo.path)),
        ];
      }
      // activeRepoId is deliberately untouched: a batch open must not steal focus.
      // With no tab open yet there is nothing to steal, so focus the first one.
      const activeRepoId = s.activeRepoId ?? openRepos[0]?.id ?? null;
      return {
        openRepos,
        activeRepoId,
        recents: recents.slice(0, 10),
        tabOrder,
      };
    });
    schedulePersist();
  },
  removeRecent: (path) => {
    set((s) => {
      const recents = s.recents.filter((r) => !samePath(r.path, path));
      // Nothing matched, so skip the write rather than persisting an identical
      // list and re-rendering every consumer of `recents`.
      if (recents.length === s.recents.length) return s;
      return { recents };
    });
    schedulePersist();
  },
  removeRepo: (id) => {
    set((s) => {
      const removed = s.openRepos.find((repo) => repo.id === id);
      // A submodule tab only exists as part of its project. Closing the project
      // closes them with it, rather than leaving orphans nested under nothing.
      const attached = removed
        ? (submoduleGroupFor(s.tabGroups, removed.path)?.repoPaths ?? [])
        : [];
      const closedKeys = new Set(attached.map(pathKey));
      const openRepos = s.openRepos.filter(
        (r) => r.id !== id && !closedKeys.has(pathKey(r.path)),
      );
      let workspace = removed
        ? removePathFromWorkspace(s.tabGroups, s.tabOrder, removed.path)
        : { groups: s.tabGroups, order: s.tabOrder };
      for (const path of attached) {
        workspace = removePathFromWorkspace(
          workspace.groups,
          workspace.order,
          path,
        );
      }
      const orderedRemaining = workspace.order.flatMap((item) => {
        if (item.type === "repo") return [item.path];
        return (
          workspace.groups.find((group) => group.id === item.id)?.repoPaths ??
          []
        );
      });
      const nextActivePath = orderedRemaining.find((path) =>
        openRepos.some((repo) => samePath(repo.path, path)),
      );
      // Closing a project also closes its submodules, so the active tab can be
      // one of those rather than the one that was clicked.
      const activeClosed = !openRepos.some(
        (repo) => repo.id === s.activeRepoId,
      );
      const activeRepoId = activeClosed
        ? (openRepos.find(
            (repo) => nextActivePath && samePath(repo.path, nextActivePath),
          )?.id ??
          openRepos[0]?.id ??
          null)
        : s.activeRepoId;
      // Closing a tab discards its unfinished message along with it.
      let commitDrafts = clearDraft(s.commitDrafts, id);
      for (const repo of s.openRepos) {
        if (closedKeys.has(pathKey(repo.path)))
          commitDrafts = clearDraft(commitDrafts, repo.id);
      }
      return {
        openRepos,
        activeRepoId,
        commitDrafts,
        tabGroups: workspace.groups,
        tabOrder: workspace.order,
        // A closed tab keeps no pin; reopening it starts unpinned.
        pinnedTabPaths: removed
          ? s.pinnedTabPaths.filter((path) => !samePath(path, removed.path))
          : s.pinnedTabPaths,
      };
    });
    schedulePersist();
  },
  setCommitDraft: (repoId, draft) => {
    set((s) => ({ commitDrafts: applyDraft(s.commitDrafts, repoId, draft) }));
  },
  clearCommitDraft: (repoId) => {
    set((s) => ({ commitDrafts: clearDraft(s.commitDrafts, repoId) }));
  },
  setActiveRepo: (id) => {
    set((s) => {
      // Switching to a tab is what "used" means, so stamp it here -- the one
      // choke point every tab click goes through.
      const repo = s.openRepos.find((candidate) => candidate.id === id);
      return {
        activeRepoId: id,
        tabLastUsedAt: repo
          ? { ...s.tabLastUsedAt, [pathKey(repo.path)]: Date.now() }
          : s.tabLastUsedAt,
      };
    });
    // Selecting a repo tab means "show me that repo", so step out of the
    // app-level picker view if it was covering the center. The picker's own tab
    // stays open, so coming back to it keeps whatever was typed there.
    const ui = useUiStore.getState();
    if (ui.centerView === "repoPicker") ui.showGraph();

    schedulePersist();
  },
  addCodeFolder: (path) => {
    set((s) => ({ codeFolders: addFolder(s.codeFolders, path) }));
    schedulePersist();
  },
  removeCodeFolder: (path) => {
    set((s) => ({ codeFolders: removeFolder(s.codeFolders, path) }));
    schedulePersist();
  },
  setPrimaryCodeFolder: (path) => {
    set((s) => ({ codeFolders: setPrimaryFolder(s.codeFolders, path) }));
    schedulePersist();
  },
  setCodeFolderLabel: (path, label) => {
    set((s) => ({ codeFolders: setFolderLabel(s.codeFolders, path, label) }));
    schedulePersist();
  },
  setCloneDirectory: (path) => {
    set({ cloneDirectory: path ? normalizePath(path) : null });
    schedulePersist();
  },
  setGitExecutable: (path) => {
    set({ gitExecutable: path });
    schedulePersist();
  },
  setGpgExecutable: (path) => {
    set({ gpgExecutable: path });
    schedulePersist();
  },
  resetSettingsGroup: (group) => {
    const keys = SETTINGS_GROUPS[group];
    const s = get();
    const snapshot: SettingsSnapshot = {};
    const next: Partial<WorkspaceState> = {};
    for (const key of keys) {
      // Capture the current value, then stage the default. The `as never`
      // casts bridge the heterogeneous value types behind the shared key set.
      (snapshot as Record<string, unknown>)[key] = s[key];
      (next as Record<string, unknown>)[key] = SETTINGS_DEFAULTS[key];
    }
    set(next as Partial<WorkspaceState>);
    schedulePersist();
    return snapshot;
  },
  resetAllSettings: () => {
    const s = get();
    const snapshot: SettingsSnapshot = {};
    const next: Partial<WorkspaceState> = {};
    for (const key of Object.keys(SETTINGS_DEFAULTS) as SettingsKey[]) {
      (snapshot as Record<string, unknown>)[key] = s[key];
      (next as Record<string, unknown>)[key] = SETTINGS_DEFAULTS[key];
    }
    set(next as Partial<WorkspaceState>);
    schedulePersist();
    return snapshot;
  },
  restoreSettings: (snapshot) => {
    set(snapshot as Partial<WorkspaceState>);
    schedulePersist();
  },
  setUpdateChannel: (channel) => {
    set({ updateChannel: channel });
    schedulePersist();
  },
  setAutoUpdate: (enabled) => {
    set({ autoUpdate: enabled });
    schedulePersist();
  },
  setAutoRestartAfterDownload: (enabled) => {
    set({ autoRestartAfterDownload: enabled });
    schedulePersist();
  },
  setBranchSwitchMode: (mode) => {
    set({ branchSwitchMode: mode });
    schedulePersist();
  },
  setMergeMethod: (method) => {
    set({ mergeMethod: method });
    schedulePersist();
  },
  setAiSelection: (provider, model) => {
    // Record the model against its provider as well as as the current one, so
    // the choice survives a switch away and back.
    set((s) => ({
      aiProvider: provider,
      aiModel: model,
      aiModels:
        provider != null && model != null
          ? { ...s.aiModels, [provider]: model }
          : s.aiModels,
    }));
    schedulePersist();
  },
  /**
   * Make a provider the default, restoring whatever model it was last set to.
   *
   * Separate from `setAiSelection` because the caller here is choosing a
   * provider, not a provider-and-model pair: the model comes from what that
   * provider already had.
   */
  setDefaultAiProvider: (provider) => {
    set((s) => ({
      aiProvider: provider,
      aiModel: provider != null ? (s.aiModels[provider] ?? null) : null,
    }));
    schedulePersist();
  },
  /**
   * Change one provider's model, leaving which provider is default alone.
   *
   * Editing a non-default provider's model must not quietly promote it -- the
   * user is preparing that provider, not switching to it. `aiModel` only moves
   * when the row being edited is already the default.
   */
  setAiModelFor: (provider, model) => {
    set((s) => {
      const aiModels = { ...s.aiModels };
      if (model != null) {
        aiModels[provider] = model;
      } else {
        delete aiModels[provider];
      }
      return {
        aiModels,
        aiModel: s.aiProvider === provider ? model : s.aiModel,
      };
    });
    schedulePersist();
  },
  /**
   * Switch AI features on or off.
   *
   * Deliberately touches nothing but this flag: the provider, its model, and
   * every credential stay exactly as they were, so turning it back on lands on
   * the same AI the user last used rather than an empty picker.
   */
  setAiEnabled: (enabled) => {
    set({ aiEnabled: enabled });
    schedulePersist();
  },
  setAiInstruction: (instruction) => {
    set({ aiInstruction: instruction });
    schedulePersist();
  },
  setOpenspecArchiveCommitTemplate: (template) => {
    set({ openspecArchiveCommitTemplate: template });
    schedulePersist();
  },
  setCommitButtonMode: (mode) => {
    set({ commitButtonMode: mode });
    schedulePersist();
  },
  setDefaultEditor: (editor) => {
    set({ defaultEditor: editor });
    schedulePersist();
  },
  setTagPushDefault: (mode) => {
    set({ tagPushDefault: mode });
    schedulePersist();
  },
  setTagPushOnCreate: (enabled) => {
    set({ tagPushOnCreate: enabled });
    schedulePersist();
  },
  setTagDeleteOnRemote: (enabled) => {
    set({ tagDeleteOnRemote: enabled });
    schedulePersist();
  },
  setDiscardResetsSubmodules: (enabled) => {
    set({ discardResetsSubmodules: enabled });
    schedulePersist();
  },
  setRepoTagOverride: (path, patch) => {
    set((s) => {
      const next = { ...s.tagOverridesByRepo };
      const current: TagOverride = { ...next[path] };

      if ("pushDefault" in patch) {
        if (patch.pushDefault == null) delete current.pushDefault;
        else current.pushDefault = patch.pushDefault;
      }
      if ("pushOnCreate" in patch) {
        if (patch.pushOnCreate == null) delete current.pushOnCreate;
        else current.pushOnCreate = patch.pushOnCreate;
      }
      if ("deleteOnRemote" in patch) {
        if (patch.deleteOnRemote == null) delete current.deleteOnRemote;
        else current.deleteOnRemote = patch.deleteOnRemote;
      }

      if (
        current.pushDefault === undefined &&
        current.pushOnCreate === undefined &&
        current.deleteOnRemote === undefined
      ) {
        delete next[path];
      } else {
        next[path] = current;
      }
      return { tagOverridesByRepo: next };
    });
    schedulePersist();
  },
  clearRepoTagOverride: (path) => {
    set((s) => {
      if (!(path in s.tagOverridesByRepo)) return s;
      const next = { ...s.tagOverridesByRepo };
      delete next[path];
      return { tagOverridesByRepo: next };
    });
    schedulePersist();
  },
  resolveTagSettings: (path) => {
    const s = get();
    const override = path ? s.tagOverridesByRepo[path] : undefined;
    return {
      pushDefault: override?.pushDefault ?? s.tagPushDefault,
      pushOnCreate: override?.pushOnCreate ?? s.tagPushOnCreate,
      deleteOnRemote: override?.deleteOnRemote ?? s.tagDeleteOnRemote,
    };
  },
  setEnableWorktrees: (enabled) => {
    // Record the deliberate choice so the first-encounter auto-enable can never
    // override it later.
    set({ enableWorktrees: enabled, worktreesSettingTouched: true });
    schedulePersist();
  },
  setEnableSpecDesk: (enabled) => {
    set({ enableSpecDesk: enabled });
    schedulePersist();
  },
  setOpenspecArchiveWithoutAsking: (skip) => {
    set({ openspecArchiveWithoutAsking: skip });
    schedulePersist();
  },
  setOpenspecDeleteWithoutAsking: (skip) => {
    set({ openspecDeleteWithoutAsking: skip });
    schedulePersist();
  },
  setWorktreeBranchCleanup: (choice) => {
    set({ worktreeBranchCleanup: choice });
    schedulePersist();
  },
  setWorktreeBranchDeleteOnRemote: (on) => {
    set({ worktreeBranchDeleteOnRemote: on });
    schedulePersist();
  },
  setRestoreTabs: (enabled) => {
    set({ restoreTabs: enabled });
    schedulePersist();
  },
  setTelemetryLevel: (level) => {
    // Records an explicit choice, which from here on outranks the build's
    // default. `telemetryLevelEffective` is updated alongside so the control
    // reflects the choice immediately, even though the reporters only pick it
    // up at the next start.
    set({ telemetryLevel: level, telemetryLevelEffective: level });
    schedulePersist();
  },
  setAutoFetch: (enabled) => {
    set({ autoFetch: enabled });
    schedulePersist();
  },
  setGhCliFallback: (enabled) => {
    set({ ghCliFallback: enabled });
    schedulePersist();
  },
  setShowTips: (enabled) => {
    set({ showTips: enabled });
    schedulePersist();
  },
  markSigningKeyPublished: (fingerprint) => {
    set((s) =>
      s.signingKeysPublished.includes(fingerprint)
        ? s
        : { signingKeysPublished: [...s.signingKeysPublished, fingerprint] }
    );
    schedulePersist();
  },
  markOnboardingSeen: () => {
    // Persisted immediately rather than on the usual debounce: the tour closing
    // is often followed by the user quitting, and a lost write replays it.
    set({ onboardingSeen: true });
    schedulePersist();
  },
  setTabLayout: (layout) => {
    set({ tabLayout: layout });
    schedulePersist();
  },
  setHorizontalTabRow: (enabled) => {
    set({ horizontalTabRow: enabled });
    schedulePersist();
  },
  setShowTabPrCount: (enabled) => {
    set({ showTabPrCount: enabled });
    schedulePersist();
  },
  setShowTabIssueCount: (enabled) => {
    set({ showTabIssueCount: enabled });
    schedulePersist();
  },
  setUiScale: (scale) => {
    set({ uiScale: clampUiScale(scale) });
    schedulePersist();
  },
  setFontFamily: (id) => {
    set({ fontFamily: id || DEFAULT_FONT_ID });
    schedulePersist();
  },
  setFontSize: (size) => {
    set({ fontSize: clampFontSize(size) });
    schedulePersist();
  },
  setFontWeight: (weight) => {
    set({ fontWeight: clampFontWeight(weight) });
    schedulePersist();
  },
  setTabAlias: (path, alias) => {
    set((s) => {
      const next = { ...s.tabAliases };
      const trimmed = alias.trim();
      if (trimmed) next[path] = trimmed;
      else delete next[path];
      return { tabAliases: next };
    });
    schedulePersist();
  },
  setTheme: (theme) => {
    set({ theme });
    schedulePersist();
  },
  setThemeMode: (mode) => {
    set({ themeMode: mode });
    schedulePersist();
  },
  setMintAccent: (enabled) => {
    set({ mintAccent: enabled });
    schedulePersist();
  },
  setShowRepoIcons: (enabled) => {
    set({ showRepoIcons: enabled });
    schedulePersist();
  },
  setTabIconOnly: (enabled) => {
    set({ tabIconOnly: enabled });
    schedulePersist();
  },
  setVerticalTabWidth: (width) => {
    set({ verticalTabWidth: clampVerticalTabWidth(width) });
    schedulePersist();
  },
  refreshRepoIcon: (path) => {
    const key = pathKey(path);
    set((s) => ({
      repoIconRevisions: {
        ...s.repoIconRevisions,
        [key]: (s.repoIconRevisions[key] ?? 0) + 1,
      },
    }));
  },
  createTabGroup: (repoPaths, options) => {
    const existingIds = new Set(get().tabGroups.map((group) => group.id));
    const requestedId = options?.id;
    const id =
      requestedId && !existingIds.has(requestedId) ? requestedId : newGroupId();
    set((s) => {
      const paths = [
        ...new Map(
          repoPaths
            .filter((path) => {
              if (!s.openRepos.some((repo) => samePath(repo.path, path)))
                return false;
              // Submodules stay with their project rather than joining a group
              // the user built.
              const group = groupForPath(s.tabGroups, path);
              return !group || !isSubmoduleGroup(group);
            })
            .map((path) => [pathKey(path), normalizePath(path)]),
        ).values(),
      ];
      if (paths.length === 0) return s;
      const positions = paths
        .map((path) => workspaceIndexForPath(s.tabGroups, s.tabOrder, path))
        .filter((index) => index >= 0);
      const insertionIndex =
        positions.length > 0 ? Math.min(...positions) : s.tabOrder.length;
      let groups = s.tabGroups;
      let order = s.tabOrder;
      for (const path of paths) {
        const next = removePathFromWorkspace(groups, order, path);
        groups = next.groups;
        order = next.order;
      }
      const group: TabGroup = {
        id,
        name: options?.name?.trim() || "New group",
        color:
          options?.color ||
          TAB_GROUP_COLORS[groups.length % TAB_GROUP_COLORS.length],
        collapsed: false,
        repoPaths: paths,
      };
      groups = [...groups, group];
      order = [...order];
      order.splice(Math.min(insertionIndex, order.length), 0, {
        type: "group",
        id,
      });
      return { tabGroups: groups, tabOrder: order };
    });
    log.info(
      `workspace: created tab group ${id} with ${repoPaths.length} repo(s)`,
    );
    schedulePersist();
    return id;
  },
  addRepoToGroup: (repoPath, groupId) => {
    set((s) => {
      const currentGroup = groupForPath(s.tabGroups, repoPath);
      if (currentGroup?.id === groupId) return s;
      // A submodule stays with its project, and a submodule group only ever
      // holds submodules, so neither end of this move is allowed.
      if (currentGroup && isSubmoduleGroup(currentGroup)) return s;
      const requested = s.tabGroups.find((group) => group.id === groupId);
      if (requested && isSubmoduleGroup(requested)) return s;
      const workspace = removePathFromWorkspace(
        s.tabGroups,
        s.tabOrder,
        repoPath,
      );
      const target = workspace.groups.find((group) => group.id === groupId);
      if (!target) return s;
      return {
        tabGroups: workspace.groups.map((group) =>
          group.id === groupId
            ? {
                ...group,
                collapsed: false,
                repoPaths: [...group.repoPaths, normalizePath(repoPath)],
              }
            : group,
        ),
        tabOrder: workspace.order,
      };
    });
    log.info(`workspace: added repo to group ${groupId}`);
    schedulePersist();
  },
  removeRepoFromGroup: (repoPath) => {
    set((s) => {
      const group = groupForPath(s.tabGroups, repoPath);
      if (!group) return s;
      // Detaching a submodule from its project would leave a tab claiming to be
      // a standalone project when it is a folder inside another one.
      if (isSubmoduleGroup(group)) return s;
      const groupIndex = s.tabOrder.findIndex(
        (item) => item.type === "group" && item.id === group.id,
      );
      const groupWillRemain = group.repoPaths.length > 1;
      const workspace = removePathFromWorkspace(
        s.tabGroups,
        s.tabOrder,
        repoPath,
      );
      const order = [...workspace.order];
      order.splice(
        Math.max(
          0,
          Math.min(groupIndex + (groupWillRemain ? 1 : 0), order.length),
        ),
        0,
        {
          type: "repo",
          path: normalizePath(repoPath),
        },
      );
      return { tabGroups: workspace.groups, tabOrder: order };
    });
    log.info("workspace: removed repo from its group");
    schedulePersist();
  },
  renameTabGroup: (groupId, name) => {
    const trimmed = name.trim();
    if (!trimmed) return;
    set((s) => ({
      tabGroups: s.tabGroups.map((group) =>
        group.id === groupId ? { ...group, name: trimmed } : group,
      ),
    }));
    schedulePersist();
  },
  setTabGroupColor: (groupId, color) => {
    set((s) => ({
      tabGroups: s.tabGroups.map((group) =>
        group.id === groupId ? { ...group, color } : group,
      ),
    }));
    schedulePersist();
  },
  toggleTabGroup: (groupId) => {
    set((s) => ({
      tabGroups: s.tabGroups.map((group) =>
        group.id === groupId
          ? { ...group, collapsed: !group.collapsed }
          : group,
      ),
    }));
    schedulePersist();
  },
  ungroupTabGroup: (groupId) => {
    set((s) => {
      const group = s.tabGroups.find((candidate) => candidate.id === groupId);
      const groupIndex = s.tabOrder.findIndex(
        (item) => item.type === "group" && item.id === groupId,
      );
      if (!group || groupIndex < 0) return s;
      // A submodule group is a fact about the folders, not an arrangement the
      // user chose, so there is nothing here to take apart.
      if (isSubmoduleGroup(group)) return s;
      const order = [...s.tabOrder];
      order.splice(
        groupIndex,
        1,
        ...group.repoPaths.map((path) => ({ type: "repo" as const, path })),
      );
      return {
        tabGroups: s.tabGroups.filter((candidate) => candidate.id !== groupId),
        tabOrder: order,
      };
    });
    schedulePersist();
  },
  removeTabGroup: (groupId) => {
    set((s) => {
      const group = s.tabGroups.find((candidate) => candidate.id === groupId);
      if (!group) return s;
      const closedKeys = new Set(group.repoPaths.map(pathKey));
      const openRepos = s.openRepos.filter(
        (repo) => !closedKeys.has(pathKey(repo.path)),
      );
      const activeRepoId = openRepos.some((repo) => repo.id === s.activeRepoId)
        ? s.activeRepoId
        : (openRepos[0]?.id ?? null);
      return {
        openRepos,
        activeRepoId,
        tabGroups: s.tabGroups.filter((candidate) => candidate.id !== groupId),
        tabOrder: s.tabOrder.filter(
          (item) => !(item.type === "group" && item.id === groupId),
        ),
        pinnedTabPaths: s.pinnedTabPaths.filter(
          (path) => !closedKeys.has(pathKey(path)),
        ),
      };
    });
    schedulePersist();
  },
  saveTabGroup: (groupId) => {
    set((s) => {
      const group = s.tabGroups.find((candidate) => candidate.id === groupId);
      if (!group) return s;
      // Reopening a saved submodule group would open the submodules without the
      // project they belong to, so these are not saveable.
      if (isSubmoduleGroup(group)) return s;
      const saved: SavedTabGroup = {
        id: group.id,
        name: group.name,
        color: group.color,
        repoPaths: [...group.repoPaths],
      };
      return {
        savedTabGroups: [
          ...s.savedTabGroups.filter((candidate) => candidate.id !== groupId),
          saved,
        ],
        pinnedSavedGroupIds: [
          groupId,
          ...s.pinnedSavedGroupIds.filter((candidate) => candidate !== groupId),
        ],
      };
    });
    schedulePersist();
  },
  createSavedTabGroup: (repoPaths, name) => {
    const trimmedName = name.trim();
    const paths = [
      ...new Map(
        repoPaths.map((path) => [pathKey(path), normalizePath(path)]),
      ).values(),
    ];
    if (!trimmedName || paths.length === 0) return null;

    const id = newGroupId();
    set((s) => ({
      savedTabGroups: [
        ...s.savedTabGroups,
        {
          id,
          name: trimmedName,
          color:
            TAB_GROUP_COLORS[s.savedTabGroups.length % TAB_GROUP_COLORS.length],
          repoPaths: paths,
        },
      ],
      pinnedSavedGroupIds: [
        id,
        ...s.pinnedSavedGroupIds.filter((candidate) => candidate !== id),
      ],
    }));
    log.info(`workspace: saved new group ${id} with ${paths.length} repo(s)`);
    schedulePersist();
    return id;
  },
  deleteSavedTabGroup: (groupId) => {
    set((s) => ({
      savedTabGroups: s.savedTabGroups.filter((group) => group.id !== groupId),
      pinnedSavedGroupIds: s.pinnedSavedGroupIds.filter((id) => id !== groupId),
    }));
    schedulePersist();
  },
  togglePinnedRepo: (repoPath) => {
    const path = normalizePath(repoPath);
    set((s) => {
      const pinned = s.pinnedRepoPaths.some((candidate) =>
        samePath(candidate, path),
      );
      return {
        pinnedRepoPaths: pinned
          ? s.pinnedRepoPaths.filter((candidate) => !samePath(candidate, path))
          : [
              path,
              ...s.pinnedRepoPaths.filter(
                (candidate) => !samePath(candidate, path),
              ),
            ],
      };
    });
    schedulePersist();
  },
  reorderPinnedRepo: (repoPath, targetPath, placeAfter = false) => {
    const from = normalizePath(repoPath);
    const to = normalizePath(targetPath);
    if (samePath(from, to)) return;
    set((s) => {
      const rest = s.pinnedRepoPaths.filter(
        (candidate) => !samePath(candidate, from),
      );
      if (rest.length === s.pinnedRepoPaths.length) return {};
      const targetIndex = rest.findIndex((candidate) => samePath(candidate, to));
      if (targetIndex === -1) return {};
      const insertAt = placeAfter ? targetIndex + 1 : targetIndex;
      return {
        pinnedRepoPaths: [
          ...rest.slice(0, insertAt),
          from,
          ...rest.slice(insertAt),
        ],
      };
    });
    schedulePersist();
  },
  togglePinnedSavedGroup: (groupId) => {
    set((s) => {
      const pinned = s.pinnedSavedGroupIds.includes(groupId);
      return {
        pinnedSavedGroupIds: pinned
          ? s.pinnedSavedGroupIds.filter((id) => id !== groupId)
          : [groupId, ...s.pinnedSavedGroupIds.filter((id) => id !== groupId)],
      };
    });
    schedulePersist();
  },
  toggleRepoPickerSection: (section) => {
    set((s) => ({
      repoPickerCollapsedSections: s.repoPickerCollapsedSections.includes(
        section,
      )
        ? s.repoPickerCollapsedSections.filter(
            (candidate) => candidate !== section,
          )
        : [...s.repoPickerCollapsedSections, section],
    }));
    schedulePersist();
  },
  sortRepoPicker: (key) => {
    set((s) => ({
      repoPickerSort:
        s.repoPickerSort.key === key
          ? { key, desc: !s.repoPickerSort.desc }
          : // A fresh column starts in the direction that is useful for it:
            // most activity first, but names and branches A to Z.
            { key, desc: key === "activity" },
    }));
    schedulePersist();
  },
  setRepoActivity: (activity) => {
    set({ repoActivity: activity, repoScanAt: Date.now() });
    schedulePersist();
  },
  setExpandedChangeFolders: (key, folders) => {
    set((s) => {
      const next = { ...s.expandedChangeFolders };
      if (folders.length === 0) delete next[key];
      else next[key] = folders;
      return { expandedChangeFolders: next };
    });
    schedulePersist();
  },
  setChangesViewMode: (mode) => {
    set({ changesViewMode: mode });
    schedulePersist();
  },
  setTabSort: (sort) => {
    // Re-picking the active sort flips it; switching sorts starts forward, so
    // Name always opens as A-Z rather than inheriting the last rule's direction.
    const s = get();
    const direction: TabSortDirection =
      sort !== "manual" &&
      s.tabSort === sort &&
      s.tabSortDirection === "forward"
        ? "reverse"
        : "forward";
    set({ tabSort: sort, tabSortDirection: direction });
    schedulePersist();
    return direction;
  },
  toggleTabPin: (repoPath) => {
    const path = normalizePath(repoPath);
    set((s) => {
      const pinned = s.pinnedTabPaths.some((candidate) =>
        samePath(candidate, path),
      );
      // Pinning pulls a tab to the front of the strip, away from its project.
      const group = groupForPath(s.tabGroups, path);
      if (!pinned && group && isSubmoduleGroup(group)) return s;
      if (pinned) {
        return {
          pinnedTabPaths: s.pinnedTabPaths.filter(
            (candidate) => !samePath(candidate, path),
          ),
        };
      }
      // Pinning pulls the repo to the front of the strip, so it cannot stay
      // nested inside a group. Lift it out and place it first in tabOrder, which
      // keeps Manual sort agreeing with what the user sees.
      const workspace = removePathFromWorkspace(s.tabGroups, s.tabOrder, path);
      return {
        tabGroups: workspace.groups,
        tabOrder: [{ type: "repo" as const, path }, ...workspace.order],
        pinnedTabPaths: [
          ...s.pinnedTabPaths.filter((candidate) => !samePath(candidate, path)),
          path,
        ],
      };
    });
    schedulePersist();
  },
  moveRepoBeside: (sourcePath, targetPath, placement) => {
    if (samePath(sourcePath, targetPath)) return;
    set((s) => {
      const sourceGroup = groupForPath(s.tabGroups, sourcePath);
      const targetGroup = groupForPath(s.tabGroups, targetPath);
      // Submodule tabs are fixed to their project: they cannot be dragged out,
      // and nothing can be dropped in beside them.
      if (sourceGroup && isSubmoduleGroup(sourceGroup)) return s;
      if (targetGroup && isSubmoduleGroup(targetGroup)) return s;
      const targetGroupId = targetGroup?.id ?? null;
      const workspace = removePathFromWorkspace(
        s.tabGroups,
        s.tabOrder,
        sourcePath,
      );
      if (targetGroupId) {
        const target = workspace.groups.find(
          (group) => group.id === targetGroupId,
        );
        if (!target) return s;
        const targetIndex = target.repoPaths.findIndex((path) =>
          samePath(path, targetPath),
        );
        const groups = workspace.groups.map((group) => {
          if (group.id !== targetGroupId) return group;
          const repoPaths = [...group.repoPaths];
          repoPaths.splice(
            targetIndex + (placement === "after" ? 1 : 0),
            0,
            normalizePath(sourcePath),
          );
          return { ...group, collapsed: false, repoPaths };
        });
        return { tabGroups: groups, tabOrder: workspace.order };
      }
      const targetIndex = workspace.order.findIndex(
        (item) => item.type === "repo" && samePath(item.path, targetPath),
      );
      if (targetIndex < 0) return s;
      const order = [...workspace.order];
      order.splice(targetIndex + (placement === "after" ? 1 : 0), 0, {
        type: "repo",
        path: normalizePath(sourcePath),
      });
      return {
        tabGroups: workspace.groups,
        tabOrder: order,
        // The pinned strip renders in pinnedTabPaths order, not tabOrder, so a
        // pinned tab only appears to move once this list moves too.
        pinnedTabPaths: reorderPinned(
          s.pinnedTabPaths,
          sourcePath,
          targetPath,
          placement === "after",
        ),
      };
    });
    schedulePersist();
  },
  moveRepoToOrder: (repoPath, requestedIndex) => {
    set((s) => {
      const sourceGroup = groupForPath(s.tabGroups, repoPath);
      // A submodule tab is fixed to its project and has no place of its own in
      // the order to move to.
      if (sourceGroup && isSubmoduleGroup(sourceGroup)) return s;
      const sourceIndex = workspaceIndexForPath(
        s.tabGroups,
        s.tabOrder,
        repoPath,
      );
      const removesOrderItem =
        !sourceGroup || sourceGroup.repoPaths.length === 1;
      let insertAt = requestedIndex;
      if (removesOrderItem && sourceIndex >= 0 && sourceIndex < requestedIndex)
        insertAt -= 1;
      const workspace = removePathFromWorkspace(
        s.tabGroups,
        s.tabOrder,
        repoPath,
      );
      const order = [...workspace.order];
      order.splice(Math.max(0, Math.min(insertAt, order.length)), 0, {
        type: "repo",
        path: normalizePath(repoPath),
      });
      return { tabGroups: workspace.groups, tabOrder: order };
    });
    schedulePersist();
  },
  adoptDisplayedTabOrder: (displayed) => {
    set((s) => {
      const key = (item: TabOrderItem) =>
        item.type === "group"
          ? `group:${item.id}`
          : `repo:${pathKey(item.path)}`;
      const shown = new Set(displayed.map(key));
      // Pinned tabs render in their own strip ahead of everything else, so they
      // are not in the displayed list. Keep them at the front, in their stored
      // sequence, and append anything else the caller did not account for.
      const missing = s.tabOrder.filter((item) => !shown.has(key(item)));
      return {
        tabOrder: [...missing, ...displayed],
        tabSort: "manual" as TabSort,
        tabSortDirection: "forward" as TabSortDirection,
      };
    });
    schedulePersist();
  },
  moveGroupToOrder: (groupId, requestedIndex) => {
    set((s) => {
      const fromIndex = s.tabOrder.findIndex(
        (item) => item.type === "group" && item.id === groupId,
      );
      if (fromIndex < 0) return s;
      let insertAt = requestedIndex;
      if (fromIndex < requestedIndex) insertAt -= 1;
      const order = [...s.tabOrder];
      const [item] = order.splice(fromIndex, 1);
      if (!item) return s;
      order.splice(Math.max(0, Math.min(insertAt, order.length)), 0, item);
      return { tabOrder: order };
    });
    schedulePersist();
  },
  finishRepoRestore: () => {
    set((s) => {
      const openKeys = new Set(s.openRepos.map((repo) => pathKey(repo.path)));
      const tabGroups = s.tabGroups
        .map((group) => ({
          ...group,
          repoPaths: group.repoPaths.filter((path) =>
            openKeys.has(pathKey(path)),
          ),
        }))
        .filter((group) => group.repoPaths.length > 0)
        // A submodule group nests under its project. If that project did not
        // reopen there is nothing to nest under, so the group is dropped and its
        // submodules fall back to ordinary tabs below.
        .filter(
          (group) =>
            group.parentPath == null || openKeys.has(pathKey(group.parentPath)),
        );
      const groupIds = new Set(tabGroups.map((group) => group.id));
      const represented = new Set(
        tabGroups.flatMap((group) => group.repoPaths.map(pathKey)),
      );
      const tabOrder = s.tabOrder.filter((item) => {
        if (item.type === "group") return groupIds.has(item.id);
        const key = pathKey(item.path);
        if (!openKeys.has(key) || represented.has(key)) return false;
        represented.add(key);
        return true;
      });
      for (const repo of s.openRepos) {
        const key = pathKey(repo.path);
        if (!represented.has(key)) {
          represented.add(key);
          tabOrder.push({ type: "repo", path: repo.path });
        }
      }
      return {
        tabGroups,
        tabOrder,
        pinnedTabPaths: s.pinnedTabPaths.filter((path) =>
          openKeys.has(pathKey(path)),
        ),
      };
    });
    schedulePersist();
  },
  reorderColumn: (id, toIndex) => {
    set((s) => {
      const from = s.columnOrder.indexOf(id);
      if (from === -1) return s;
      const next = [...s.columnOrder];
      next.splice(from, 1);
      next.splice(Math.max(0, Math.min(toIndex, next.length)), 0, id);
      return { columnOrder: next };
    });
    schedulePersist();
  },
  toggleColumn: (id) => {
    set((s) => ({
      hiddenColumns: s.hiddenColumns.includes(id)
        ? s.hiddenColumns.filter((c) => c !== id)
        : [...s.hiddenColumns, id],
    }));
    schedulePersist();
  },
  resetColumns: () => {
    set({
      columnOrder: DEFAULT_COLUMN_ORDER,
      hiddenColumns: [],
      columnWidths: {},
    });
    schedulePersist();
  },
  setColumnWidth: (id, width) => {
    set((s) => ({
      columnWidths: { ...s.columnWidths, [id]: clampColumnWidth(id, width) },
    }));
    schedulePersist();
  },
  resetColumnWidth: (id) => {
    set((s) => {
      const columnWidths = { ...s.columnWidths };
      delete columnWidths[id];
      return { columnWidths };
    });
    schedulePersist();
  },
  setLeftPanelWidth: (width) => {
    set({ leftPanelWidth: clampLeftPanelWidth(width) });
    schedulePersist();
  },
  setRightPanelWidth: (width) => {
    set({ rightPanelWidth: clampRightPanelWidth(width) });
    schedulePersist();
  },
  setDrawerHeight: (height) => {
    set({ drawerHeight: clampDrawerHeight(height) });
    schedulePersist();
  },
  setDrawerListWidth: (width) => {
    set({ drawerListWidth: clampDrawerListWidth(width) });
    schedulePersist();
  },
  setChangesSplit: (split) => {
    set({ changesSplit: clampChangesSplit(split) });
    schedulePersist();
  },
  setCommitDescLines: (lines) => {
    set({ commitDescLines: clampCommitDescLines(lines) });
    schedulePersist();
  },
  setConflictSideSplit: (split) => {
    set({ conflictSideSplit: clampConflictSideSplit(split) });
    schedulePersist();
  },
  setConflictResultSplit: (split) => {
    set({ conflictResultSplit: clampConflictResultSplit(split) });
    schedulePersist();
  },
  resetWorkspaceLayout: () => {
    set({
      leftPanelWidth: DEFAULT_LEFT_PANEL_WIDTH,
      rightPanelWidth: DEFAULT_RIGHT_PANEL_WIDTH,
      drawerHeight: DEFAULT_DRAWER_HEIGHT,
      drawerListWidth: DEFAULT_DRAWER_LIST_WIDTH,
      changesSplit: DEFAULT_CHANGES_SPLIT,
      commitDescLines: DEFAULT_COMMIT_DESC_LINES,
      conflictSideSplit: DEFAULT_CONFLICT_SIDE_SPLIT,
      conflictResultSplit: DEFAULT_CONFLICT_RESULT_SPLIT,
      verticalTabWidth: DEFAULT_VERTICAL_TAB_WIDTH,
    });
    schedulePersist();
  },
  setChangeSizeDisplay: (display) => {
    set({ changeSizeDisplay: display });
    schedulePersist();
  },
  setShowChangeIndicator: (enabled) => {
    set({ showChangeIndicator: enabled });
    schedulePersist();
  },
  setShowChangeLineCounts: (enabled) => {
    set({ showChangeLineCounts: enabled });
    schedulePersist();
  },
  setShowGraphAvatars: (enabled) => {
    set({ showGraphAvatars: enabled });
    schedulePersist();
  },

  hydrate: async () => {
    const settings = unwrap(await commands.getSettings());
    // Resolved by the backend rather than derived here, because "unset" only
    // becomes a level once the version and update channel are known. Failing
    // this read must not read as consent to the widest level, so it falls back
    // to the narrowest one that is still a default.
    const telemetryLevelEffective = await commands
      .getTelemetryLevel()
      .then((r) => (r.status === "ok" ? r.data : "reports"))
      .catch(() => "reports" as const);
    if (!get().hydrated) {
      const tabGroups = deserializeTabGroups(settings.tab_groups);
      const savedTabGroups = deserializeSavedTabGroups(
        settings.saved_tab_groups,
      );
      set({
        recents: (settings.recents ?? []).map((recent) => ({
          ...recent,
          path: normalizePath(recent.path),
        })),
        codeFolders: deserializeCodeFolders(
          settings.code_folders,
          settings.code_folder,
        ),
        cloneDirectory: settings.clone_directory ?? null,
        gitExecutable: settings.git_executable ?? "",
        gpgExecutable: settings.gpg_executable ?? "",
        updateChannel: settings.update_channel === "beta" ? "beta" : "stable",
        autoUpdate: settings.auto_update !== false,
        // Defaults off, so an absent value must read as false -- unlike
        // autoUpdate above, whose default is on.
        autoRestartAfterDownload: settings.auto_restart_after_download === true,
        branchSwitchMode: settings.branch_switch_mode ?? "auto_stash",
        mergeMethod: settings.merge_method ?? "merge",
        aiProvider: settings.ai_provider ?? null,
        aiModel: settings.ai_model ?? null,
        // Upgrade path: an install from before per-provider models has only the
        // single ai_provider/ai_model pair. Seed the map from it so the user's
        // existing choice survives and nobody reconfigures. Stored values win
        // where both exist.
        aiModels: normalizeAiModels(
          settings.ai_models,
          settings.ai_provider,
          settings.ai_model,
        ),
        // Absent means on: a settings file written before this flag existed
        // belongs to a user whose AI was working, and must keep working.
        aiEnabled: settings.ai_enabled !== false,
        aiInstruction: settings.ai_instruction ?? null,
        openspecArchiveCommitTemplate: settings.openspec_archive_commit_template ?? null,
        columnOrder: normalizeOrder(settings.column_layout?.order),
        hiddenColumns: normalizeHidden(settings.column_layout?.hidden),
        columnWidths: normalizeColumnWidths(settings.column_layout?.widths),
        leftPanelWidth: clampLeftPanelWidth(
          settings.left_panel_width ?? DEFAULT_LEFT_PANEL_WIDTH,
        ),
        rightPanelWidth: clampRightPanelWidth(
          settings.right_panel_width ?? DEFAULT_RIGHT_PANEL_WIDTH,
        ),
        drawerHeight: clampDrawerHeight(
          settings.drawer_height ?? DEFAULT_DRAWER_HEIGHT,
        ),
        drawerListWidth: clampDrawerListWidth(
          settings.drawer_commit_list_width ?? DEFAULT_DRAWER_LIST_WIDTH,
        ),
        changesSplit: clampChangesSplit(
          settings.changes_split ?? DEFAULT_CHANGES_SPLIT,
        ),
        commitDescLines: clampCommitDescLines(
          settings.commit_description_lines ?? DEFAULT_COMMIT_DESC_LINES,
        ),
        conflictSideSplit: clampConflictSideSplit(
          settings.conflict_side_split ?? DEFAULT_CONFLICT_SIDE_SPLIT,
        ),
        conflictResultSplit: clampConflictResultSplit(
          settings.conflict_result_split ?? DEFAULT_CONFLICT_RESULT_SPLIT,
        ),
        changeSizeDisplay:
          settings.change_size_display === "row" ? "row" : "column",
        showChangeIndicator: settings.show_change_indicator ?? true,
        showChangeLineCounts: settings.show_change_line_counts ?? false,
        showGraphAvatars: settings.show_graph_avatars ?? false,
        commitButtonMode:
          settings.commit_button_mode === "commit_push"
            ? "commit_push"
            : "commit",
        // An editor id this build does not know (older/newer settings file)
        // falls back to VS Code rather than leaving the button unusable.
        defaultEditor: EDITOR_KINDS.has(settings.default_editor as EditorKind)
          ? (settings.default_editor as EditorKind)
          : DEFAULT_EDITOR,
        tagPushDefault: normalizeTagPushDefault(settings.tag_push_default),
        tagPushOnCreate: settings.tag_push_on_create ?? false,
        tagDeleteOnRemote: settings.tag_delete_on_remote ?? false,
        discardResetsSubmodules: settings.discard_resets_submodules ?? true,
        tagOverridesByRepo: normalizeTagOverrides(
          settings.tag_overrides_by_repo,
        ),
        enableWorktrees: settings.enable_worktrees ?? false,
        worktreesSettingTouched: settings.worktrees_setting_touched ?? false,
        // Absent means on: a settings file written before this flag existed
        // belongs to a user who could already see the Specs surfaces.
        enableSpecDesk: settings.enable_spec_desk !== false,
        // Both default to asking: an absent flag in an older settings file must
        // mean "confirm", never "delete silently".
        openspecArchiveWithoutAsking:
          settings.openspec_archive_without_asking === true,
        openspecDeleteWithoutAsking:
          settings.openspec_delete_without_asking === true,
        worktreeBranchCleanup: readBranchCleanup(
          settings.worktree_branch_cleanup,
        ),
        worktreeBranchDeleteOnRemote:
          settings.worktree_branch_delete_on_remote === true,
        restoreTabs: settings.restore_tabs ?? true,
        autoFetch: settings.auto_fetch ?? true,
        ghCliFallback: settings.gh_cli_fallback ?? true,
        // Absent means on: a settings file written before this flag existed
        // belongs to someone who has been seeing the tips all along, so hiding
        // them on upgrade would look like the sidebar lost content.
        showTips: settings.show_tips ?? true,
        // Absent stays absent: `null` is a meaningful state here ("never
        // chosen"), so it must not be coerced to a level. Doing so would freeze
        // today's default into the settings file on the next save and break the
        // narrowing to `reports` at 1.0.
        telemetryLevel: settings.telemetry_level ?? null,
        telemetryLevelEffective,
        onboardingSeen: settings.onboarding_seen ?? false,
        signingKeysPublished: settings.signing_keys_published ?? [],
        uiScale:
          settings.ui_scale != null
            ? clampUiScale(settings.ui_scale)
            : DEFAULT_UI_SCALE,
        fontFamily: settings.font_family ?? DEFAULT_FONT_ID,
        fontSize:
          settings.font_size != null
            ? clampFontSize(settings.font_size)
            : DEFAULT_FONT_SIZE,
        fontWeight:
          settings.font_weight != null
            ? clampFontWeight(settings.font_weight)
            : DEFAULT_FONT_WEIGHT,
        tabAliases: normalizeAliases(settings.tab_aliases),
        theme: normalizeTheme(settings.theme),
        themeMode: normalizeThemeMode(settings.theme_mode),
        mintAccent: settings.mint_accent ?? true,
        showRepoIcons: settings.show_repo_icons ?? true,
        tabIconOnly: settings.tab_icon_only ?? false,
        verticalTabWidth: clampVerticalTabWidth(
          settings.vertical_tab_width ?? DEFAULT_VERTICAL_TAB_WIDTH,
        ),
        tabLayout:
          settings.tab_layout === "horizontal" ? "horizontal" : "vertical",
        horizontalTabRow: settings.horizontal_tab_row ?? false,
        showTabPrCount: settings.show_tab_pr_count ?? false,
        showTabIssueCount: settings.show_tab_issue_count ?? false,
        tabGroups,
        tabOrder: deserializeTabOrder(settings.tab_order, tabGroups),
        tabSort: normalizeTabSort(settings.tab_sort),
        tabSortDirection: normalizeTabSortDirection(
          settings.tab_sort_direction,
        ),
        tabLastUsedAt: normalizeTabLastUsedAt(settings.tab_last_used_at),
        pinnedTabPaths: (settings.pinned_tab_paths ?? []).map(normalizePath),
        savedTabGroups,
        pinnedRepoPaths: (settings.pinned_repo_paths ?? []).map(normalizePath),
        pinnedSavedGroupIds:
          settings.pinned_saved_group_ids ??
          savedTabGroups.slice(0, 3).map((group) => group.id),
        repoPickerCollapsedSections: normalizeRepoPickerSections(
          settings.repo_picker_collapsed_sections,
        ),
        repoPickerSort: normalizeRepoPickerSort(settings.repo_picker_sort),
        repoScanAt: normalizeScanAt(settings.repo_scan_at),
        expandedChangeFolders: normalizeExpandedChangeFolders(
          settings.expanded_change_folders,
        ),
        changesViewMode:
          settings.changes_view_mode === "list" ? "list" : "tree",
        hydrated: true,
      });
    }
    return settings;
  },

  reloadSettings: async () => {
    const settings = unwrap(await commands.getSettings());
    // Deliberately narrow: only the settings another window can change while
    // this one is open. A full re-read would also pull tab groups, order and
    // panel sizes, clobbering live layout state in this window with whatever
    // the last debounced write happened to contain.
    set({
      aiProvider: settings.ai_provider ?? null,
      aiModel: settings.ai_model ?? null,
      aiModels: normalizeAiModels(
        settings.ai_models,
        settings.ai_provider,
        settings.ai_model,
      ),
      aiEnabled: settings.ai_enabled !== false,
      aiInstruction: settings.ai_instruction ?? null,
      // Appearance is a global preference, not per-window layout, so copying it
      // cannot clobber anything this window owns. Without it a window that was
      // already open keeps the theme, font and zoom it started with while the
      // rest of the app has moved on.
      theme: normalizeTheme(settings.theme),
      themeMode: normalizeThemeMode(settings.theme_mode),
      mintAccent: settings.mint_accent ?? true,
      fontFamily: settings.font_family ?? DEFAULT_FONT_ID,
      fontSize:
        settings.font_size != null
          ? clampFontSize(settings.font_size)
          : DEFAULT_FONT_SIZE,
      fontWeight:
        settings.font_weight != null
          ? clampFontWeight(settings.font_weight)
          : DEFAULT_FONT_WEIGHT,
      uiScale:
        settings.ui_scale != null
          ? clampUiScale(settings.ui_scale)
          : DEFAULT_UI_SCALE,
      // The Desk opens files and runs openspec actions, so it has to honour the
      // same editor choice and the same "don't ask me again" answers the user
      // just gave in the main window -- otherwise it re-prompts for something
      // they already dismissed, or opens the wrong editor.
      defaultEditor: EDITOR_KINDS.has(settings.default_editor as EditorKind)
        ? (settings.default_editor as EditorKind)
        : DEFAULT_EDITOR,
      enableSpecDesk: settings.enable_spec_desk !== false,
      openspecArchiveWithoutAsking:
        settings.openspec_archive_without_asking === true,
      openspecDeleteWithoutAsking:
        settings.openspec_delete_without_asking === true,
      worktreeBranchCleanup: readBranchCleanup(settings.worktree_branch_cleanup),
      worktreeBranchDeleteOnRemote:
        settings.worktree_branch_delete_on_remote === true,
      openspecArchiveCommitTemplate:
        settings.openspec_archive_commit_template ?? null,
    });
  },
}));

// Commit selection, open diffs and conflicts all belong to one repo. Whenever the
// active repo changes - tab click, close, group close - drop them so the graph
// never queries the new repo for the old repo's objects.
useWorkspaceStore.subscribe((s, prev) => {
  if (s.activeRepoId !== prev.activeRepoId)
    useUiStore.getState().resetForRepoSwitch();
});

export { primaryFirst } from "@/lib/codeFolders";
export type { CodeFolder } from "@/lib/codeFolders";

/**
 * The starred folder's path, or null when no folder is watched. This is the
 * default destination for clones and new repositories.
 */
export function usePrimaryCodeFolder(): string | null {
  return useWorkspaceStore((s) => primaryPathOf(s.codeFolders));
}

export function useActiveRepo(): RepoInfo | null {
  const openRepos = useWorkspaceStore((s) => s.openRepos);
  const activeRepoId = useWorkspaceStore((s) => s.activeRepoId);
  return openRepos.find((r) => r.id === activeRepoId) ?? null;
}

// Dev-only: `openRepos` holds live repo handles that cannot be serialized, so a
// hot reload that re-evaluates this module would drop every open tab -- and the
// debounced persist would then write the empty list back to settings.json. Carry
// the state across reloads so editing a component never closes the user's repos.
// Production builds never run this.
if (import.meta.hot) {
  const carried = import.meta.hot.data.workspaceState as
    | WorkspaceState
    | undefined;
  if (carried) {
    useWorkspaceStore.setState(carried, true);
  }
  import.meta.hot.dispose((data) => {
    data.workspaceState = useWorkspaceStore.getState();
  });
}
