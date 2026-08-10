import {
  Fragment,
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  type CSSProperties,
  type DragEvent,
  type ReactNode,
} from "react";
import {
  ArrowDown,
  ArrowUp,
  Check,
  ChevronLeft,
  ChevronRight,
  FolderOpen,
  GitBranch,
  ImageIcon,
  Layers3,
  Pencil,
  Pin,
  PinOff,
  Plus,
  Save,
  Settings2,
  Trash2,
  Ungroup,
  X,
} from "lucide-react";
import { toast } from "sonner";
import type { RepoInfo } from "@/lib/bindings";
import { commands } from "@/lib/bindings";
import { cn } from "@/lib/utils";
import { EditorGlyph, editorLabel } from "@/lib/editors";
import { normalizePath } from "@/lib/paths";
import {
  isSubmoduleGroup,
  TAB_GROUP_COLORS,
  useWorkspaceStore,
  type TabDropPlacement,
  type TabGroup,
  type TabOrderItem,
} from "@/stores/workspaceStore";
import { arrangeTabs } from "@/lib/tabSorting";
import { useTabStatusStore } from "@/stores/tabStatusStore";
import { useUiStore } from "@/stores/uiStore";
import { useBranches, useRepoTabStatus } from "@/hooks/useGitQueries";
import { useGitMutations } from "@/hooks/useGitMutations";
import { branchSync } from "@/lib/branchActions";
import { pullNeedsChoice } from "@/lib/syncPreview";
import { Button } from "@/components/ui/button";
import { PendingMenuItem } from "@/components/ui/pending-menu-item";
import { Input } from "@/components/ui/input";
import { FormDialog } from "@/components/ui/form-dialog";
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuLabel,
  ContextMenuSeparator,
  ContextMenuSub,
  ContextMenuSubContent,
  ContextMenuSubTrigger,
  ContextMenuTrigger,
} from "@/components/ui/context-menu";
import {
  Tooltip,
  TooltipButton,
  TooltipContent,
  TooltipHint,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import { RepoIconDialog } from "@/components/domain/RepoIconDialog";
import { onTitleBarDoubleClick } from "@/components/domain/titleBarDrag";

export type TabOrientation = "horizontal" | "vertical";

const MAX_HORIZONTAL_TAB_WIDTH = 208;
const ICON_ONLY_TAB_WIDTH = 38;
const VERTICAL_ICON_ONLY_WIDTH = 72;
// Under this width a horizontal tab is too tight for the numbered status
// badges, so they collapse to pulsing glyphs.
const STATUS_NUMBERS_MIN_WIDTH = 116;

type DragItem = { type: "repo"; path: string } | { type: "group"; id: string };

type DropTarget =
  | { type: "order"; index: number }
  | { type: "repo"; path: string; placement: TabDropPlacement | "group" }
  | { type: "group"; id: string }
  // A group being dragged over another group. The thin gaps between tabs are
  // too small to aim a whole group at, so a group also lands on either side of
  // another group depending on which half of it the pointer is over.
  | { type: "groupSide"; id: string; placement: TabDropPlacement };

interface RenameTarget {
  type: "tab" | "group";
  id: string;
  value: string;
  fallback?: string;
  /** Group was just created by this dialog, so cancelling should undo it. */
  isNewGroup?: boolean;
}

function pathKey(path: string): string {
  return normalizePath(path).toLowerCase();
}

function samePath(left: string, right: string): boolean {
  return pathKey(left) === pathKey(right);
}

function RenameDialog({
  target,
  onClose,
  onSave,
}: {
  target: RenameTarget;
  onClose: () => void;
  onSave: (value: string) => void;
}) {
  const [value, setValue] = useState(target.value);
  const title = target.type === "group" ? "Rename group" : "Rename tab";
  const label = target.type === "group" ? "Group name" : "Tab name";

  const canSave = target.type !== "group" || value.trim() !== "";
  const save = () => {
    if (canSave) onSave(value);
  };

  return (
    <FormDialog
      open
      onOpenChange={(open) => !open && onClose()}
      icon={<Pencil size={15} strokeWidth={1.9} />}
      title={title}
      submitLabel="Save"
      canSubmit={canSave}
      onSubmit={save}
    >
      <div className="grid gap-1.5">
        <label className="text-2xs font-semibold text-sub">{label}</label>
        <Input
          value={value}
          onChange={(event) => setValue(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === "Enter") save();
          }}
          placeholder={target.fallback}
          className="h-auto bg-background py-1.5 text-xs"
          autoFocus
        />
        {target.type === "tab" && (
          <p className="text-2xs text-muted-foreground">
            Leave blank to use the folder name.
          </p>
        )}
      </div>
    </FormDialog>
  );
}

function groupStyle(color: string): CSSProperties {
  return { "--tab-group-color": color } as CSSProperties;
}

function DropGap({
  orientation,
  active,
  label,
}: {
  orientation: TabOrientation;
  active: boolean;
  label: string;
}) {
  return (
    <div
      aria-hidden="true"
      className={cn(
        "gw-tab-drop-gap grid flex-none place-items-center overflow-hidden rounded-[5px] border-dashed text-2xs font-semibold text-accent-text transition-[width,height,margin,border-color,background-color] duration-150",
        orientation === "horizontal"
          ? active
            ? "mx-0.5 h-full w-24 border border-primary/60 bg-soft"
            : "h-full w-0 border-0"
          : active
            ? "my-0.5 h-8 w-full border border-primary/60 bg-soft"
            : "h-0 w-full border-0",
      )}
    >
      {active ? label : ""}
    </div>
  );
}

function useScrollEdges(enabled: boolean) {
  const ref = useRef<HTMLDivElement>(null);
  const [edges, setEdges] = useState({ start: false, end: false });
  const [viewportWidth, setViewportWidth] = useState(0);

  const measure = useCallback(() => {
    const node = ref.current;
    if (!node) return;
    setViewportWidth(node.clientWidth);
    const max = node.scrollWidth - node.clientWidth;
    setEdges((current) => {
      const next = {
        start: node.scrollLeft > 1,
        end: node.scrollLeft < max - 1,
      };
      return current.start === next.start && current.end === next.end
        ? current
        : next;
    });
  }, []);

  useLayoutEffect(() => {
    const node = ref.current;
    if (!enabled || !node) {
      setEdges({ start: false, end: false });
      setViewportWidth(0);
      return;
    }
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(node);
    for (const child of Array.from(node.children)) observer.observe(child);
    const mutation = new MutationObserver(measure);
    mutation.observe(node, { childList: true });
    node.addEventListener("scroll", measure, { passive: true });
    return () => {
      observer.disconnect();
      mutation.disconnect();
      node.removeEventListener("scroll", measure);
    };
  }, [enabled, measure]);

  const scrollBy = (direction: -1 | 1) => {
    const node = ref.current;
    if (!node) return;
    node.scrollBy({
      left: direction * Math.max(160, node.clientWidth * 0.7),
      behavior: "smooth",
    });
  };

  return { ref, edges, scrollBy, viewportWidth };
}

function ScrollArrow({
  side,
  show,
  onClick,
}: {
  side: "left" | "right";
  show: boolean;
  onClick: () => void;
}) {
  if (!show) return null;
  return (
    <TooltipButton
      onClick={onClick}
      className={cn(
        "flex h-full w-5 flex-none items-center justify-center bg-background text-sub hover:bg-panel2 hover:text-foreground",
        side === "left" ? "border-r border-border" : "border-l border-border",
      )}
      tooltip={side === "left" ? "Scroll tabs left" : "Scroll tabs right"}
    >
      {side === "left" ? (
        <ChevronLeft size={13} strokeWidth={2.2} />
      ) : (
        <ChevronRight size={13} strokeWidth={2.2} />
      )}
    </TooltipButton>
  );
}

function findRepo(openRepos: RepoInfo[], path: string): RepoInfo | undefined {
  return openRepos.find((repo) => samePath(repo.path, path));
}

function orderedPaths(order: TabOrderItem[], groups: TabGroup[]): string[] {
  return order.flatMap((item) =>
    item.type === "repo"
      ? [item.path]
      : (groups.find((group) => group.id === item.id)?.repoPaths ?? []),
  );
}

function RepoTabIcon({
  repoPath,
  color,
  name,
  forceFallback,
}: {
  repoPath: string;
  color: string;
  name: string;
  forceFallback: boolean;
}) {
  const revision = useWorkspaceStore(
    (state) => state.repoIconRevisions[pathKey(repoPath)] ?? 0,
  );
  const [dataUrl, setDataUrl] = useState<string | null>(null);

  useEffect(() => {
    let active = true;
    commands
      .getRepoIcon(repoPath)
      .then((result) => {
        if (active)
          setDataUrl(
            result.status === "ok" ? (result.data?.data_url ?? null) : null,
          );
      })
      .catch(() => {
        if (active) setDataUrl(null);
      });
    return () => {
      active = false;
    };
  }, [repoPath, revision]);

  if (!dataUrl) {
    if (!forceFallback) return null;
    return (
      <span
        className="grid size-5 flex-none place-items-center rounded-[4px] text-[9px] font-bold uppercase text-background"
        style={{ background: color }}
        aria-hidden="true"
      >
        {name.trim().charAt(0) || "R"}
      </span>
    );
  }

  return (
    <span className="grid size-5 flex-none place-items-center overflow-hidden rounded-[4px] bg-background">
      <img
        src={dataUrl}
        alt=""
        className="size-full object-cover"
        onError={() => setDataUrl(null)}
      />
    </span>
  );
}

function RepoTabPreview({
  repo,
  name,
  orientation,
}: {
  repo: RepoInfo;
  name: string;
  orientation: TabOrientation;
}) {
  return (
    <TooltipContent
      side={orientation === "vertical" ? "right" : "bottom"}
      className="w-80 max-w-[calc(100vw-16px)] overflow-hidden p-0"
    >
      <div className="px-3 py-2.5">
        <div className="flex items-center gap-2">
          <span className="min-w-0 flex-1 truncate text-xs font-semibold text-foreground">
            {name}
          </span>
          <span className="flex max-w-32 items-center gap-1 font-mono text-[10px] text-sub">
            <GitBranch size={10} strokeWidth={2} />
            <span className="truncate">{repo.head_branch ?? "No branch"}</span>
          </span>
        </div>
        <div className="mt-0.5 break-all font-mono text-[10px] leading-4 text-muted-foreground">
          {repo.path}
        </div>
      </div>
    </TooltipContent>
  );
}

/**
 * The trailing "Add a repository" tab. The Open-repository button in the app bar
 * (or rail) creates it via showRepoPicker(); it does not exist until then. Once
 * opened it behaves like a real tab: it stays put while the user clicks over to a
 * repository and back, and only goes away when they close it or finish adding a
 * repository. It is deliberately NOT part of tabOrder -- it never reorders,
 * groups, or persists, so it lives outside the drag system entirely. It is still
 * marked draggable so we can catch the drag attempt and, instead of moving it,
 * nudge the picker view with a little "nuh uh, over here" wiggle.
 */
function AddRepoTab({
  orientation,
  iconOnly,
  width,
}: {
  orientation: TabOrientation;
  iconOnly: boolean;
  width: number;
}) {
  const open = useUiStore((state) => state.repoPickerOpen);
  const active = useUiStore((state) => state.centerView === "repoPicker");
  const showRepoPicker = useUiStore((state) => state.showRepoPicker);
  const closeRepoPicker = useUiStore((state) => state.closeRepoPicker);
  const wiggleRepoPicker = useUiStore((state) => state.wiggleRepoPicker);
  const [hovered, setHovered] = useState(false);

  const nudge = (event: DragEvent<HTMLElement>) => {
    // Cancel the drag before it starts, then wiggle the picker into view.
    event.preventDefault();
    if (!active) showRepoPicker();
    wiggleRepoPicker();
  };

  // The tab only exists once the picker has been opened from the app bar.
  if (!open) return null;

  const showName = !iconOnly || (orientation === "horizontal" && hovered);
  const horizontalWidth =
    iconOnly && hovered ? MAX_HORIZONTAL_TAB_WIDTH : width;

  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <div
          role="button"
          tabIndex={0}
          draggable
          onDragStart={nudge}
          onClick={() => showRepoPicker()}
          onKeyDown={(event) => {
            if (event.key === "Enter" || event.key === " ") {
              event.preventDefault();
              showRepoPicker();
            }
          }}
          onMouseEnter={() => setHovered(true)}
          onMouseLeave={() => setHovered(false)}
          onMouseDown={(event) => {
            if (event.button === 1) event.preventDefault();
          }}
          onAuxClick={(event) => {
            if (event.button !== 1) return;
            event.preventDefault();
            event.stopPropagation();
            closeRepoPicker();
          }}
          aria-label="Add a repository"
          aria-current={active ? "page" : undefined}
          className={cn(
            "group/add relative flex min-w-0 cursor-pointer items-center gap-[7px] overflow-hidden border-border text-xs transition-[border-color,background-color,color]",
            orientation === "horizontal"
              ? "h-full flex-none border-l px-2.5"
              : "mx-1.5 my-1 h-[31px] flex-none rounded-[5px] border border-dashed px-2",
            iconOnly && orientation === "horizontal" && !hovered && "justify-center px-1",
            active
              ? cn(
                  "gw-tab-active bg-[color:color-mix(in_srgb,var(--gw-accent)_12%,var(--gw-panel2))] font-semibold text-accent-text",
                  orientation === "horizontal"
                    ? "gw-tab-active-top"
                    : "gw-tab-active-side",
                )
              : "text-sub hover:bg-panel2 hover:text-foreground",
          )}
          style={
            orientation === "horizontal" ? { width: horizontalWidth } : undefined
          }
        >
          <Plus size={13} strokeWidth={2.2} className="flex-none" />
          {showName && (
            <span className="min-w-0 flex-1 overflow-hidden text-ellipsis whitespace-nowrap">
              Add a repository
            </span>
          )}
          <TooltipButton
            draggable={false}
            onClick={(event) => {
              event.stopPropagation();
              closeRepoPicker();
            }}
            className={cn(
              "ml-auto flex flex-none items-center justify-center overflow-hidden rounded text-muted-foreground transition-[width,opacity,margin] duration-150 hover:bg-panel3 hover:text-foreground",
              "w-0 opacity-0 group-hover/add:w-[15px] group-hover/add:opacity-100 group-focus-within/add:w-[15px] group-focus-within/add:opacity-100",
              orientation === "horizontal" &&
                "-ml-[7px] group-hover/add:ml-0 group-focus-within/add:ml-0",
            )}
            tooltip="Close this tab"
          >
            <X size={11} />
          </TooltipButton>
        </div>
      </TooltipTrigger>
      <TooltipContent side={orientation === "vertical" ? "right" : "bottom"}>
        Add a repository
      </TooltipContent>
    </Tooltip>
  );
}

function StatusBadge({
  icon,
  count,
  color,
  label,
  collapsed,
}: {
  icon: ReactNode;
  count: number;
  color: string;
  label: string;
  /** Shrunk tab: hide the number and show only the glyph. */
  collapsed: boolean;
}) {
  return (
    <TooltipHint label={`${count} ${label}`}>
      <span
        className="flex flex-none items-center gap-0.5 font-mono text-[10px] font-semibold leading-none tabular-nums"
        style={{ color }}
        aria-label={`${count} ${label}`}
      >
        {icon}
        {!collapsed && count}
      </span>
    </TooltipHint>
  );
}

/**
 * Publishes one repository's change count for the "Has changes" sort. This is
 * mounted once per open repository, outside the tab strip's own markup, so a
 * repo keeps reporting even while its tab is unmounted -- collapsing a group
 * hides its member tabs, and without this the group's count would fall to zero
 * and the group would jump to the bottom of the strip. Renders nothing; the
 * queries it reads are shared with the visible tabs.
 */
function ChangeCountReporter({ repo }: { repo: RepoInfo }) {
  const { ahead, behind, uncommitted } = useRepoTabStatus(repo.id);
  const path = repo.path;

  useEffect(() => {
    useTabStatusStore
      .getState()
      .reportChangeCount(path, { ahead, behind, uncommitted });
  }, [path, ahead, behind, uncommitted]);

  return null;
}

/**
 * The push / pull / uncommitted totals for every repository in a group, shown
 * on the group header. Collapsed, it is the only sign of pending work inside;
 * expanded, it summarises what the member tabs are each showing.
 */
function GroupStatusIcons({
  group,
  collapsed,
}: {
  group: TabGroup;
  /** Shrunk header: show the glyphs without their numbers. */
  collapsed: boolean;
}) {
  const breakdowns = useTabStatusStore((state) => state.changeBreakdowns);
  const total = group.repoPaths.reduce(
    (sum, path) => {
      const counts = breakdowns[pathKey(path)];
      if (!counts) return sum;
      return {
        ahead: sum.ahead + counts.ahead,
        behind: sum.behind + counts.behind,
        uncommitted: sum.uncommitted + counts.uncommitted,
      };
    },
    { ahead: 0, behind: 0, uncommitted: 0 },
  );

  if (total.ahead === 0 && total.behind === 0 && total.uncommitted === 0)
    return null;

  return (
    <span className="ml-auto flex flex-none items-center gap-1.5 pl-1">
      {total.ahead > 0 && (
        <StatusBadge
          icon={<ArrowUp size={10} strokeWidth={2.4} />}
          count={total.ahead}
          color="var(--gw-blue)"
          label="to push in this group"
          collapsed={collapsed}
        />
      )}
      {total.behind > 0 && (
        <StatusBadge
          icon={<ArrowDown size={10} strokeWidth={2.4} />}
          count={total.behind}
          color="var(--gw-red)"
          label="to pull in this group"
          collapsed={collapsed}
        />
      )}
      {total.uncommitted > 0 && (
        <StatusBadge
          icon={<Pencil size={9} strokeWidth={2.4} />}
          count={total.uncommitted}
          color="var(--gw-amber)"
          label="uncommitted in this group"
          collapsed={collapsed}
        />
      )}
    </span>
  );
}

/**
 * The push / pull / uncommitted badges a repository tab shows on its right side.
 * Each badge only appears when its count is above zero, so a clean repo shows
 * nothing. When the tab is shrunk to icons the numbers drop away and a pulsing
 * green ring wraps the cluster to keep the pending state noticeable -- except on
 * the active tab, which the user is already looking at.
 */
function TabStatusIcons({
  repoId,
  collapsed,
  pulse,
}: {
  repoId: string;
  collapsed: boolean;
  pulse: boolean;
}) {
  const { ahead, behind, uncommitted } = useRepoTabStatus(repoId);

  if (ahead === 0 && behind === 0 && uncommitted === 0) return null;

  return (
    <span
      className={cn(
        "flex flex-none items-center gap-1",
        collapsed ? "px-0.5" : "gap-1.5",
        pulse && "wyrm-tab-status-pulse",
      )}
    >
      {ahead > 0 && (
        <StatusBadge
          icon={<ArrowUp size={11} strokeWidth={2.4} />}
          count={ahead}
          color="var(--gw-blue)"
          label="to push"
          collapsed={collapsed}
        />
      )}
      {behind > 0 && (
        <StatusBadge
          icon={<ArrowDown size={11} strokeWidth={2.4} />}
          count={behind}
          color="var(--gw-red)"
          label="to pull"
          collapsed={collapsed}
        />
      )}
      {uncommitted > 0 && (
        <StatusBadge
          icon={<Pencil size={10} strokeWidth={2.4} />}
          count={uncommitted}
          color="var(--gw-amber)"
          label="uncommitted"
          collapsed={collapsed}
        />
      )}
    </span>
  );
}

/**
 * Get-changes for one tab's repository, without switching to it first. Lives in
 * its own component because the mutation hook is per repository and the tab
 * menu is rendered inside a loop.
 *
 * Always enabled: the behind count only reflects the last fetch, so a repo that
 * looks up to date may still have waiting changes. Pulling is how you find out.
 */
function TabPullItem({ repo, name }: { repo: RepoInfo; name: string }) {
  const m = useGitMutations(repo.id);
  const { ahead, behind } = useRepoTabStatus(repo.id);
  const setActiveRepo = useWorkspaceStore((s) => s.setActiveRepo);
  const openRemoteSync = useUiStore((s) => s.openRemoteSync);
  const branches = useBranches(repo.id);

  // Work on both sides means a plain pull would invent a merge commit. Send
  // that to the sync modal instead -- but it reads the ACTIVE repo, so this
  // switches to the tab first rather than showing another repo's branches.
  const head = branches.data?.local.find((b) => b.is_head);
  const canChooseSync = pullNeedsChoice({
    upstream: head?.upstream,
    ahead,
    behind,
  });

  return (
    <PendingMenuItem
      icon={<ArrowDown size={13} strokeWidth={2} />}
      label={behind > 0 ? `Pull ${behind} into ${name}` : `Pull into ${name}`}
      pendingLabel="Pulling…"
      pending={m.pull.isPending}
      onRun={() => {
        if (canChooseSync) {
          setActiveRepo(repo.id);
          openRemoteSync(head!.upstream!, head!.name);
          return;
        }
        m.pull.mutate();
      }}
    />
  );
}

/**
 * Send this tab's work, shown only when there is work to send. Unlike Pull,
 * which is always offered because the behind count can be stale, the ahead
 * count is local knowledge and trustworthy, so nothing-to-push means no item.
 *
 * `never_pushed` counts as something to push: a branch that has never reached
 * the remote has no upstream to be ahead of, so its count is zero while all of
 * its commits are still waiting.
 */
function TabPushItem({ repo, name }: { repo: RepoInfo; name: string }) {
  const m = useGitMutations(repo.id);
  const branches = useBranches(repo.id);
  const head = branches.data?.local.find((b) => b.is_head);
  const sync = head ? branchSync(head) : null;

  if (!sync) return null;
  const isNew = sync.marker === "new";
  if (sync.ahead === 0 && !isNew) return null;

  return (
    <PendingMenuItem
      icon={<ArrowUp size={13} strokeWidth={2} />}
      label={isNew ? `Publish ${name}` : `Push ${sync.ahead} from ${name}`}
      pendingLabel="Pushing…"
      pending={m.push.isPending}
      onRun={() => {
        // Behind as well as ahead: a plain push is refused as non-fast-forward,
        // so hand it to the same choice modal the toolbar uses. That modal reads
        // the active repository, so this tab has to become active first or it
        // would offer to force-push the wrong one.
        if (sync.behind > 0) {
          useWorkspaceStore.getState().setActiveRepo(repo.id);
          useUiStore.getState().openModal("push-choice");
          return;
        }
        m.push.mutate();
      }}
    />
  );
}

/**
 * Hand this tab's repository to Explorer or to the editor picked in Settings.
 * Both commands take a repository id, so the tab does not have to become active
 * first -- right-clicking a background tab opens that repository, not this one.
 */
function TabExternalItems({ repo }: { repo: RepoInfo }) {
  const m = useGitMutations(repo.id);
  const defaultEditor = useWorkspaceStore((state) => state.defaultEditor);

  return (
    <>
      <PendingMenuItem
        icon={<FolderOpen size={13} strokeWidth={2} />}
        label="Open in Explorer"
        pendingLabel="Opening…"
        pending={m.revealInFileManager.isPending}
        onRun={() => m.revealInFileManager.mutate()}
      />
      <PendingMenuItem
        icon={<EditorGlyph kind={defaultEditor} size={13} className="flex-none" />}
        label={`Open in ${editorLabel(defaultEditor)}`}
        pendingLabel="Opening…"
        pending={m.openInEditor.isPending}
        onRun={() => m.openInEditor.mutate(defaultEditor)}
      />
    </>
  );
}

export function RepositoryTabs({
  orientation,
}: {
  orientation: TabOrientation;
}) {
  const openRepos = useWorkspaceStore((state) => state.openRepos);
  const activeRepoId = useWorkspaceStore((state) => state.activeRepoId);
  const tabAliases = useWorkspaceStore((state) => state.tabAliases);
  const showRepoIcons = useWorkspaceStore((state) => state.showRepoIcons);
  const tabIconOnly = useWorkspaceStore((state) => state.tabIconOnly);
  const verticalTabWidth = useWorkspaceStore((state) => state.verticalTabWidth);
  const tabGroups = useWorkspaceStore((state) => state.tabGroups);
  const tabOrder = useWorkspaceStore((state) => state.tabOrder);
  const tabSort = useWorkspaceStore((state) => state.tabSort);
  const tabSortDirection = useWorkspaceStore((state) => state.tabSortDirection);
  const pinnedTabPaths = useWorkspaceStore((state) => state.pinnedTabPaths);
  const changeCounts = useTabStatusStore((state) => state.changeCounts);
  const savedTabGroups = useWorkspaceStore((state) => state.savedTabGroups);
  const repoPickerOpen = useUiStore((state) => state.repoPickerOpen);
  const [renaming, setRenaming] = useState<RenameTarget | null>(null);
  const [iconRepo, setIconRepo] = useState<RepoInfo | null>(null);
  const [hoveredRepoPath, setHoveredRepoPath] = useState<string | null>(null);
  const [dragItem, setDragItem] = useState<DragItem | null>(null);
  const [dropTarget, setDropTarget] = useState<DropTarget | null>(null);
  const draggedGroupRef = useRef<string | null>(null);
  const {
    ref: scrollRef,
    edges,
    scrollBy,
    viewportWidth,
  } = useScrollEdges(orientation === "horizontal");

  const repoName = (repo: RepoInfo) => tabAliases[repo.path] ?? repo.name;
  const isSaved = (groupId: string) =>
    savedTabGroups.some((group) => group.id === groupId);
  const isPinned = (path: string) =>
    pinnedTabPaths.some((candidate) => samePath(candidate, path));

  const arrangement = useMemo(
    () =>
      arrangeTabs({
        order: tabOrder,
        groups: tabGroups,
        sort: tabSort,
        direction: tabSortDirection,
        pinned: pinnedTabPaths,
        aliases: tabAliases,
        names: Object.fromEntries(
          openRepos.map((repo) => [pathKey(repo.path), repo.name]),
        ),
        changeCounts,
      }),
    [
      tabOrder,
      tabGroups,
      tabSort,
      tabSortDirection,
      pinnedTabPaths,
      tabAliases,
      openRepos,
      changeCounts,
    ],
  );
  // Counts outlive their tabs on purpose (see ChangeCountReporter), so closing a
  // repository is what forgets its count.
  useEffect(() => {
    useTabStatusStore
      .getState()
      .pruneChangeCounts(openRepos.map((repo) => repo.path));
  }, [openRepos]);

  // Pinned tabs render in their own strip; drag-to-reorder still addresses
  // positions in tabOrder, so gaps only appear in the sortable remainder.
  const displayOrder = arrangement.rest;
  const pinnedOrder = arrangement.pinned;
  const verticalIconOnly =
    orientation === "vertical" && verticalTabWidth <= VERTICAL_ICON_ONLY_WIDTH;
  const effectiveIconOnly = tabIconOnly || verticalIconOnly;
  const showTabIcons = showRepoIcons || effectiveIconOnly;

  // The "Add a repository" tab claims a full tab's width once it is open, so it
  // has to share the same budget or it pushes the repo tabs into overflow.
  const visibleRepoCount = tabOrder.reduce((count, item) => {
    if (item.type === "repo") return count + (findRepo(openRepos, item.path) ? 1 : 0);
    const group = tabGroups.find((candidate) => candidate.id === item.id);
    if (!group || group.collapsed) return count;
    return count + group.repoPaths.filter((path) => findRepo(openRepos, path)).length;
  }, repoPickerOpen ? 1 : 0);
  const groupHeaderBudget = tabOrder.reduce((width, item) => {
    if (item.type !== "group") return width;
    const group = tabGroups.find((candidate) => candidate.id === item.id);
    if (!group) return width;
    // Room for the rolled-up push / pull / uncommitted badges, so adding them
    // does not quietly steal width from the repository tabs.
    const rollup = group.repoPaths.some(
      (path) => (changeCounts[pathKey(path)] ?? 0) > 0,
    )
      ? 56
      : 0;
    return width + Math.min(128, 48 + group.name.length * 6) + rollup;
  }, 0);
  const minimumNamedTabWidth = showTabIcons ? 74 : 52;
  const adaptiveHorizontalTabWidth = tabIconOnly
    ? ICON_ONLY_TAB_WIDTH
    : viewportWidth > 0 && visibleRepoCount > 0
      ? Math.max(
          minimumNamedTabWidth,
          Math.min(
            MAX_HORIZONTAL_TAB_WIDTH,
            Math.floor((Math.max(0, viewportWidth - groupHeaderBudget) / visibleRepoCount)),
          ),
        )
      : MAX_HORIZONTAL_TAB_WIDTH;

  const setTarget = (next: DropTarget | null) => {
    setDropTarget((current) =>
      JSON.stringify(current) === JSON.stringify(next) ? current : next,
    );
  };

  const closeRepo = (repo: RepoInfo) => {
    // Submodule tabs go with their project, so the backend has to release them
    // too -- the store drops them from the workspace on its own.
    const attachedPaths =
      tabGroups.find(
        (group) =>
          group.parentPath != null && samePath(group.parentPath, repo.path),
      )?.repoPaths ?? [];
    void commands.closeRepo(repo.id);
    for (const path of attachedPaths) {
      const attachedRepo = findRepo(openRepos, path);
      if (attachedRepo) void commands.closeRepo(attachedRepo.id);
    }
    useWorkspaceStore.getState().removeRepo(repo.id);
    toast.success(
      attachedPaths.length > 0
        ? `Closed ${repoName(repo)} and its ${attachedPaths.length === 1 ? "part" : `${attachedPaths.length} parts`}`
        : `Closed ${repoName(repo)}`,
    );
  };

  const closeGroup = (group: TabGroup) => {
    for (const path of group.repoPaths) {
      const repo = findRepo(openRepos, path);
      if (repo) void commands.closeRepo(repo.id);
    }
    useWorkspaceStore.getState().removeTabGroup(group.id);
    toast.success(
      `Closed ${group.name} and ${group.repoPaths.length} repositories`,
    );
  };

  const createGroup = (paths: string[]) => {
    const id = useWorkspaceStore.getState().createTabGroup(paths);
    setRenaming({
      type: "group",
      id,
      value: "New group",
      isNewGroup: true,
      fallback: `${paths.length} repositories`,
    });
  };

  const finishDrag = () => {
    setDragItem(null);
    setTarget(null);
  };

  /**
   * Under Name or Has-changes the strip is a computed view, so a move written to
   * the manual order would not show up. Dragging a tab is the user asking to
   * arrange things themselves, so switch to Manual -- freezing the arrangement
   * they can see first, or every other tab would jump as the sort let go.
   *
   * Returns true when the switch happened, so the caller can say so. Callers
   * that resolve a position in tabOrder must do it AFTER this: adopting the
   * displayed arrangement renumbers the order.
   */
  const adoptOrderForDrag = (): boolean => {
    if (tabSort === "manual") return false;
    useWorkspaceStore.getState().adoptDisplayedTabOrder(displayOrder);
    return true;
  };

  /** Appended to a move's toast when the drag also turned the sort off. */
  const sortSwitchNote = " Tab order switched to Manual so your arrangement sticks.";

  const sameOrderItem = (a: TabOrderItem, b: TabOrderItem) =>
    a.type === "group"
      ? b.type === "group" && b.id === a.id
      : b.type === "repo" && samePath(b.path, a.path);

  const startRepoDrag = (event: DragEvent<HTMLElement>, repo: RepoInfo) => {
    event.stopPropagation();
    event.dataTransfer.effectAllowed = "move";
    event.dataTransfer.setData("text/plain", repo.path);
    setDragItem({ type: "repo", path: repo.path });
    setTarget(null);
    toast.info(
      `Moving ${repoName(repo)}. Use an edge to reorder or the center to group.`,
    );
  };

  const startGroupDrag = (event: DragEvent<HTMLElement>, group: TabGroup) => {
    event.stopPropagation();
    event.dataTransfer.effectAllowed = "move";
    event.dataTransfer.setData("text/plain", `group:${group.id}`);
    draggedGroupRef.current = group.id;
    setDragItem({ type: "group", id: group.id });
    setTarget(null);
    toast.info(
      `Moving ${group.name} with all ${group.repoPaths.length} repositories`,
    );
  };

  /**
   * Drop into the gap that sits in front of `anchor` -- or at the very end when
   * there is no anchor. Addressed by anchor rather than by index because
   * switching to Manual renumbers tabOrder, so any index taken beforehand would
   * point at the wrong tab.
   */
  const dropOnOrder = (anchor: TabOrderItem | null) => {
    if (!dragItem) return;
    const switched = adoptOrderForDrag();
    const store = useWorkspaceStore.getState();
    const resolved = anchor
      ? store.tabOrder.findIndex((item) => sameOrderItem(anchor, item))
      : -1;
    const insertAt = resolved < 0 ? store.tabOrder.length : resolved;
    const note = switched ? sortSwitchNote : "";

    if (dragItem.type === "group") {
      store.moveGroupToOrder(dragItem.id, insertAt);
      const group = tabGroups.find((candidate) => candidate.id === dragItem.id);
      toast.success(`${group?.name ?? "Group"} moved.${note}`);
    } else {
      store.moveRepoToOrder(dragItem.path, insertAt);
      const repo = findRepo(openRepos, dragItem.path);
      toast.success(
        `${repo ? repoName(repo) : "Repository"} moved between tabs.${note}`,
      );
    }
    finishDrag();
  };

  /**
   * Land a dragged group on one side of another item in the manual order. The
   * gaps between tabs are only a couple of pixels wide, which is fine for
   * nudging a single tab but far too small to aim a whole group at, so groups
   * also drop onto the near half of whatever they are hovering.
   */
  const dropGroupBeside = (
    groupId: string,
    isTarget: (item: TabOrderItem) => boolean,
    placement: TabDropPlacement,
  ) => {
    const group = tabGroups.find((candidate) => candidate.id === groupId);
    const wasSorted = adoptOrderForDrag();
    // Resolved after the freeze: adopting the displayed arrangement renumbers
    // tabOrder, so an index taken beforehand would point at the wrong tab.
    const store = useWorkspaceStore.getState();
    const targetIndex = store.tabOrder.findIndex(isTarget);
    if (targetIndex < 0) {
      finishDrag();
      return;
    }
    store.moveGroupToOrder(
      groupId,
      placement === "before" ? targetIndex : targetIndex + 1,
    );
    toast.success(
      `${group?.name ?? "Group"} moved.${wasSorted ? sortSwitchNote : ""}`,
    );
    finishDrag();
  };

  const dropGroupBesideGroup = (
    groupId: string,
    targetGroupId: string,
    placement: TabDropPlacement,
  ) =>
    dropGroupBeside(
      groupId,
      (item) => item.type === "group" && item.id === targetGroupId,
      placement,
    );

  const dropGroupBesideRepo = (
    groupId: string,
    targetPath: string,
    placement: TabDropPlacement,
  ) =>
    dropGroupBeside(
      groupId,
      (item) => item.type === "repo" && samePath(item.path, targetPath),
      placement,
    );

  const dropOnRepo = (
    targetPath: string,
    placement: TabDropPlacement | "group",
  ) => {
    if (dragItem?.type !== "repo" || samePath(dragItem.path, targetPath))
      return;
    const sourceRepo = findRepo(openRepos, dragItem.path);
    const targetRepo = findRepo(openRepos, targetPath);
    const targetGroup = tabGroups.find((group) =>
      group.repoPaths.some((path) => samePath(path, targetPath)),
    );
    const sourceGroup = tabGroups.find((group) =>
      group.repoPaths.some((path) => samePath(path, dragItem.path)),
    );

    if (placement !== "group") {
      // Grouping is unaffected by the sort -- membership is not positional --
      // so only the before/after moves adopt the displayed order. Both tabs are
      // addressed by path, which survives the renumbering, so no index to redo.
      const switched = adoptOrderForDrag();
      useWorkspaceStore
        .getState()
        .moveRepoBeside(dragItem.path, targetPath, placement);
      toast.success(
        `${sourceRepo ? repoName(sourceRepo) : "Repository"} moved ${placement} ${targetRepo ? repoName(targetRepo) : "the tab"}.${switched ? sortSwitchNote : ""}`,
      );
    } else if (targetGroup) {
      if (sourceGroup?.id === targetGroup.id) {
        toast.info(
          `${sourceRepo ? repoName(sourceRepo) : "Repository"} is already in ${targetGroup.name}`,
        );
      } else {
        useWorkspaceStore
          .getState()
          .addRepoToGroup(dragItem.path, targetGroup.id);
        toast.success(
          `${sourceRepo ? repoName(sourceRepo) : "Repository"} added to ${targetGroup.name}`,
        );
      }
    } else {
      createGroup([targetPath, dragItem.path]);
    }
    finishDrag();
  };

  const dropOnGroup = (group: TabGroup) => {
    if (dragItem?.type !== "repo") return;
    const currentGroup = tabGroups.find((candidate) =>
      candidate.repoPaths.some((path) => samePath(path, dragItem.path)),
    );
    const repo = findRepo(openRepos, dragItem.path);
    if (currentGroup?.id === group.id) {
      toast.info(
        `${repo ? repoName(repo) : "Repository"} is already in ${group.name}`,
      );
    } else {
      useWorkspaceStore.getState().addRepoToGroup(dragItem.path, group.id);
      toast.success(
        `${repo ? repoName(repo) : "Repository"} added to ${group.name}`,
      );
    }
    finishDrag();
  };

  const closeOthers = (keepPath: string) => {
    for (const repo of openRepos) {
      if (!samePath(repo.path, keepPath)) {
        void commands.closeRepo(repo.id);
        useWorkspaceStore.getState().removeRepo(repo.id);
      }
    }
    toast.success("Closed the other repositories");
  };

  const closeAfter = (path: string) => {
    const paths = orderedPaths(tabOrder, tabGroups);
    const index = paths.findIndex((candidate) => samePath(candidate, path));
    if (index < 0) return;
    const closing = paths.slice(index + 1);
    for (const repoPath of closing) {
      const repo = findRepo(openRepos, repoPath);
      if (repo) {
        void commands.closeRepo(repo.id);
        useWorkspaceStore.getState().removeRepo(repo.id);
      }
    }
    toast.success(
      `Closed ${closing.length} ${closing.length === 1 ? "repository" : "repositories"}`,
    );
  };

  const renderRepoTab = (
    repo: RepoInfo,
    group: TabGroup | null,
    pinned = false,
  ) => {
    const target =
      dropTarget?.type === "repo" && samePath(dropTarget.path, repo.path)
        ? dropTarget.placement
        : null;
    const inGroup = group != null;
    // A submodule tab is part of the project above it, not a tab of its own, so
    // it neither travels nor accepts anything dropped onto it. The project's own
    // tab is a member of that same group but is NOT attached: it is the thing
    // being hung from, and stays as draggable and droppable as any other tab.
    const isGroupParent =
      group?.parentPath != null && samePath(group.parentPath, repo.path);
    const attached = group != null && isSubmoduleGroup(group) && !isGroupParent;
    const active = repo.id === activeRepoId;
    const groupsForMenu = tabGroups.filter(
      (candidate) => candidate.id !== group?.id,
    );
    const pathOrder = orderedPaths(tabOrder, tabGroups);
    const pathIndex = pathOrder.findIndex((path) => samePath(path, repo.path));
    const hovered = hoveredRepoPath != null && samePath(hoveredRepoPath, repo.path);
    // Pinned horizontal tabs carry double width, so their name still fits even
    // when the rest of the strip has collapsed to icons.
    const showName =
      !effectiveIconOnly ||
      (orientation === "horizontal" && (hovered || (pinned && !tabIconOnly)));
    // A pinned tab is one the user singled out, so it keeps twice the room when
    // the strip squeezes -- enough to stay readable while the others shrink to
    // icons. Capped at the normal maximum so it never grows past a full tab.
    const pinnedFloor = Math.min(
      MAX_HORIZONTAL_TAB_WIDTH,
      (tabIconOnly ? ICON_ONLY_TAB_WIDTH : minimumNamedTabWidth) * 2,
    );
    const horizontalWidth =
      tabIconOnly && hovered
        ? MAX_HORIZONTAL_TAB_WIDTH
        : pinned
          ? Math.max(adaptiveHorizontalTabWidth, pinnedFloor)
          : adaptiveHorizontalTabWidth;
    // Below the size that fits a numbered badge the status icons drop their
    // counts and pulse instead. Named vertical tabs always have room; horizontal
    // tabs collapse once the shared width budget squeezes them narrow.
    const statusCollapsed = showName
      ? orientation === "horizontal" && horizontalWidth < STATUS_NUMBERS_MIN_WIDTH
      : true;
    const horizontalDropGap =
      orientation === "horizontal" && (target === "before" || target === "after") ? 100 : 0;
    const tabStyle: CSSProperties | undefined = orientation === "horizontal"
      ? { ...(group ? groupStyle(group.color) : {}), width: horizontalWidth }
      : group
        ? groupStyle(group.color)
        : undefined;

    return (
      <div
        key={repo.path}
        className={cn(
          orientation === "horizontal"
            ? "flex h-full flex-none flex-row"
            : "flex w-full flex-none flex-col",
          orientation === "horizontal" && (tabIconOnly || dragItem) &&
            "transition-[width] duration-100 ease-out",
        )}
        style={orientation === "horizontal" ? { width: horizontalWidth + horizontalDropGap } : undefined}
        onMouseEnter={() => setHoveredRepoPath(repo.path)}
        onMouseLeave={() =>
          setHoveredRepoPath((current) =>
            current != null && samePath(current, repo.path) ? null : current,
          )
        }
        onDragOver={(event) => {
          // Nothing lands on a submodule tab: it has no place of its own in the
          // order, and it cannot take on members.
          if (attached) return;
          // A group dragged over a loose tab lands on the near side of it. Over
          // a grouped tab it is the parent group's section that decides, so let
          // the event bubble there instead.
          if (dragItem?.type === "group") {
            if (inGroup) return;
            event.preventDefault();
            event.stopPropagation();
            const rect = event.currentTarget.getBoundingClientRect();
            const ratio =
              orientation === "horizontal"
                ? (event.clientX - rect.left) / rect.width
                : (event.clientY - rect.top) / rect.height;
            setTarget({
              type: "repo",
              path: repo.path,
              placement: ratio < 0.5 ? "before" : "after",
            });
            return;
          }
          if (dragItem?.type !== "repo" || samePath(dragItem.path, repo.path))
            return;
          event.preventDefault();
          event.stopPropagation();
          const tab =
            event.currentTarget.querySelector<HTMLElement>("[data-repo-tab]");
          if (!tab) return;
          const rect = tab.getBoundingClientRect();
          const pointer =
            orientation === "horizontal"
              ? event.clientX - rect.left
              : event.clientY - rect.top;
          const ratio =
            pointer / (orientation === "horizontal" ? rect.width : rect.height);
          setTarget({
            type: "repo",
            path: repo.path,
            placement: ratio < 0.3 ? "before" : ratio > 0.7 ? "after" : "group",
          });
        }}
        onDragLeave={(event) => {
          if (
            !event.currentTarget.contains(event.relatedTarget as Node | null) &&
            target
          )
            setTarget(null);
        }}
        onDrop={(event) => {
          if (attached) return;
          if (dragItem?.type === "group") {
            if (inGroup) return;
            event.preventDefault();
            event.stopPropagation();
            dropGroupBesideRepo(
              dragItem.id,
              repo.path,
              target === "before" ? "before" : "after",
            );
            return;
          }
          event.preventDefault();
          event.stopPropagation();
          dropOnRepo(repo.path, target ?? "group");
        }}
      >
        <DropGap
          orientation={orientation}
          active={target === "before"}
          label="Move here"
        />
        <Tooltip>
          <ContextMenu>
            <ContextMenuTrigger asChild>
              <TooltipTrigger asChild>
                <div
              data-repo-tab
              draggable={!attached}
              onDragStart={
                attached ? undefined : (event) => startRepoDrag(event, repo)
              }
              onDragEnd={attached ? undefined : finishDrag}
              onClick={() =>
                useWorkspaceStore.getState().setActiveRepo(repo.id)
              }
              onMouseDown={(event) => {
                if (event.button === 1) event.preventDefault();
              }}
              onAuxClick={(event) => {
                if (event.button !== 1) return;
                event.preventDefault();
                event.stopPropagation();
                closeRepo(repo);
              }}
              className={cn(
                "group/repo relative flex min-w-0 cursor-pointer items-center gap-[7px] overflow-hidden text-xs transition-[border-color,background-color,color]",
                orientation === "horizontal"
                  ? "h-full w-full flex-none border-l px-2.5"
                  : effectiveIconOnly
                    ? "h-[31px] w-full flex-none justify-center rounded-[5px] border px-1"
                    : attached
                      ? // Indented past a normal group member: this tab sits
                        // one level inside the project it belongs to.
                        "h-[31px] w-full flex-none cursor-pointer rounded-[5px] border px-2 pl-9"
                      : // The project heading its own submodule group reads as a
                        // top-level tab, not as a member indented inside one.
                        inGroup && !isGroupParent
                        ? "h-[31px] w-full flex-none rounded-[5px] border px-2 pl-7"
                        : "h-[31px] w-full flex-none rounded-[5px] border px-2 pl-2",
                effectiveIconOnly && orientation === "horizontal" && !hovered &&
                  "justify-center px-1",
                inGroup && orientation === "horizontal"
                  ? "border-[color:color-mix(in_srgb,var(--tab-group-color)_20%,var(--gw-border))]"
                  : "border-border",
                // The selected tab gets a tinted fill plus an accent edge, so
                // which repository you are looking at is obvious at a glance.
                active
                  ? cn(
                      "gw-tab-active font-semibold text-foreground",
                      inGroup
                        ? "bg-[color:color-mix(in_srgb,var(--tab-group-color)_14%,var(--gw-panel2))]"
                        : "bg-[color:color-mix(in_srgb,var(--gw-accent)_12%,var(--gw-panel2))]",
                      orientation === "horizontal"
                        ? "gw-tab-active-top"
                        : "gw-tab-active-side",
                    )
                  : "text-sub hover:bg-panel2 hover:text-foreground",
                target === "group" &&
                  "z-10 border-primary! bg-soft! shadow-[0_0_0_2px_var(--gw-accent-soft)]",
              )}
              style={tabStyle}
              aria-label={repoName(repo)}
            >
              {pinned && (
                <Pin
                  size={10}
                  strokeWidth={2.4}
                  aria-label="Pinned"
                  className="flex-none rotate-45 text-accent-text"
                />
              )}
              {showTabIcons ? (
                <RepoTabIcon
                  repoPath={repo.path}
                  color={group?.color ?? "var(--gw-accent)"}
                  name={repoName(repo)}
                  forceFallback={effectiveIconOnly}
                />
              ) : null}
              {showName && (
                <span className="min-w-0 flex-1 overflow-hidden text-ellipsis whitespace-nowrap">
                  {repoName(repo)}
                </span>
              )}
              <span className="ml-auto flex flex-none items-center gap-1.5">
                <TabStatusIcons
                  repoId={repo.id}
                  collapsed={statusCollapsed}
                  pulse={statusCollapsed && !active}
                />
                <TooltipButton
                  draggable={false}
                  onClick={(event) => {
                    event.stopPropagation();
                    closeRepo(repo);
                  }}
                  className={cn(
                    "flex flex-none items-center justify-center overflow-hidden rounded text-muted-foreground transition-[width,opacity,margin] duration-150 hover:bg-panel3 hover:text-foreground",
                    "w-0 opacity-0 group-hover/repo:w-[15px] group-hover/repo:opacity-100 group-focus-within/repo:w-[15px] group-focus-within/repo:opacity-100",
                    orientation === "horizontal" &&
                      "-ml-[7px] group-hover/repo:ml-0 group-focus-within/repo:ml-0",
                  )}
                  tooltip="Close repository"
                >
                  <X size={11} />
                </TooltipButton>
              </span>
                </div>
              </TooltipTrigger>
            </ContextMenuTrigger>
            <ContextMenuContent className="w-52">
            <TabPullItem repo={repo} name={repoName(repo)} />
            <TabPushItem repo={repo} name={repoName(repo)} />
            <ContextMenuSeparator />
            <ContextMenuItem
              onSelect={() => {
                useWorkspaceStore.getState().setActiveRepo(repo.id);
                useUiStore.getState().showSettings("repository");
              }}
            >
              <Settings2 size={13} strokeWidth={2} />
              Repository settings
            </ContextMenuItem>
            <ContextMenuSeparator />
            <TabExternalItems repo={repo} />
            <ContextMenuSeparator />
            <ContextMenuItem
              onSelect={() =>
                setRenaming({
                  type: "tab",
                  id: repo.path,
                  value: tabAliases[repo.path] ?? "",
                  fallback: repo.name,
                })
              }
            >
              <Pencil size={13} strokeWidth={2} />
              Rename tab
            </ContextMenuItem>
            <ContextMenuItem onSelect={() => setIconRepo(repo)}>
              <ImageIcon size={13} strokeWidth={2} />
              Set icon
            </ContextMenuItem>
            {/* A submodule tab is fixed to its project, so the moves that would
                pull it out are left off its menu entirely. */}
            {!attached && (
              <>
            <ContextMenuItem
              onSelect={() => {
                const wasPinned = isPinned(repo.path);
                useWorkspaceStore.getState().toggleTabPin(repo.path);
                toast.success(
                  wasPinned
                    ? `${repoName(repo)} unpinned`
                    : group
                      ? `${repoName(repo)} pinned to the front, out of ${group.name}`
                      : `${repoName(repo)} pinned to the front`,
                );
              }}
            >
              {isPinned(repo.path) ? (
                <PinOff size={13} strokeWidth={2} />
              ) : (
                <Pin size={13} strokeWidth={2} />
              )}
              {isPinned(repo.path) ? "Unpin tab" : "Pin tab"}
            </ContextMenuItem>
            <ContextMenuItem onSelect={() => createGroup([repo.path])}>
              <Layers3 size={13} strokeWidth={2} />
              Create new group
            </ContextMenuItem>
            {groupsForMenu.length > 0 && (
              <ContextMenuSub>
                <ContextMenuSubTrigger>
                  <Layers3 size={13} strokeWidth={2} />
                  Add to group
                </ContextMenuSubTrigger>
                <ContextMenuSubContent className="w-44">
                  {groupsForMenu.map((candidate) => (
                    <ContextMenuItem
                      key={candidate.id}
                      onSelect={() => {
                        useWorkspaceStore
                          .getState()
                          .addRepoToGroup(repo.path, candidate.id);
                        toast.success(
                          `${repoName(repo)} added to ${candidate.name}`,
                        );
                      }}
                    >
                      <span
                        className="size-2 rounded-full"
                        style={{ background: candidate.color }}
                      />
                      {candidate.name}
                    </ContextMenuItem>
                  ))}
                </ContextMenuSubContent>
              </ContextMenuSub>
            )}
            {group && (
              <ContextMenuItem
                onSelect={() => {
                  useWorkspaceStore.getState().removeRepoFromGroup(repo.path);
                  toast.success(`${repoName(repo)} removed from ${group.name}`);
                }}
              >
                <Ungroup size={13} strokeWidth={2} />
                Remove from group
              </ContextMenuItem>
            )}
              </>
            )}
            <ContextMenuSeparator />
            <ContextMenuItem onSelect={() => closeRepo(repo)}>
              <X size={13} strokeWidth={2} />
              Close tab
            </ContextMenuItem>
            <ContextMenuItem
              disabled={openRepos.length <= 1}
              onSelect={() => closeOthers(repo.path)}
            >
              Close other tabs
            </ContextMenuItem>
            <ContextMenuItem
              disabled={pathIndex < 0 || pathIndex === pathOrder.length - 1}
              onSelect={() => closeAfter(repo.path)}
            >
              Close tabs after this
            </ContextMenuItem>
            </ContextMenuContent>
          </ContextMenu>
          <RepoTabPreview
            repo={repo}
            name={repoName(repo)}
            orientation={orientation}
          />
        </Tooltip>
        <DropGap
          orientation={orientation}
          active={target === "after"}
          label="Move here"
        />
      </div>
    );
  };

  const renderGroup = (group: TabGroup) => {
    const groupTarget =
      dropTarget?.type === "group" && dropTarget.id === group.id;
    // Which side of this group a dragged group would land on, if any.
    const sideTarget =
      dropTarget?.type === "groupSide" && dropTarget.id === group.id
        ? dropTarget.placement
        : null;
    const saved = isSaved(group.id);
    // A submodule group is built for the user and belongs to the tab above it.
    // It cannot be dragged, renamed, saved or taken apart -- only folded shut.
    const attached = isSubmoduleGroup(group);
    const parentRepo = group.parentPath
      ? findRepo(openRepos, group.parentPath)
      : null;
    const parentLabel = parentRepo ? repoName(parentRepo) : group.name;
    const groupRepos = group.repoPaths.map((path) => ({
      path,
      repo: findRepo(openRepos, path),
    }));
    // An attached group holds its project as its first member and draws it as
    // the row everything else hangs from, so the nested list leaves it out.
    const nestedPaths = attached
      ? group.repoPaths.filter(
          (path) => group.parentPath == null || !samePath(path, group.parentPath),
        )
      : group.repoPaths;
    return (
      <Tooltip key={group.id}>
        <ContextMenu>
          <ContextMenuTrigger asChild>
            <section
              data-tab-group={group.id}
              className={cn(
                "gw-tab-group flex border-[color:var(--tab-group-color)] transition-opacity",
                orientation === "horizontal"
                  ? "h-full min-w-0 flex-row border-b-2 bg-[color:color-mix(in_srgb,var(--tab-group-color)_5%,transparent)]"
                  : "relative w-full flex-none flex-col border-l-2 pl-[3px]",
                orientation === "horizontal" && "flex-none",
                dragItem?.type === "group" &&
                  dragItem.id === group.id &&
                  "opacity-35",
                // Landing edge for another group being dragged onto this one.
                sideTarget === "before" &&
                  (orientation === "horizontal"
                    ? "border-l-2 border-l-primary"
                    : "border-t-2 border-t-primary"),
                sideTarget === "after" &&
                  (orientation === "horizontal"
                    ? "border-r-2 border-r-primary"
                    : "border-b-2 border-b-primary"),
              )}
              style={groupStyle(group.color)}
              onDragOver={(event) => {
                // A submodule group sits where its project is, so nothing lands
                // beside it.
                if (attached) return;
                // Only whole-group drags land here; a repo drag belongs to the
                // header button or a member tab underneath.
                if (dragItem?.type !== "group" || dragItem.id === group.id)
                  return;
                event.preventDefault();
                event.stopPropagation();
                const rect = event.currentTarget.getBoundingClientRect();
                const ratio =
                  orientation === "horizontal"
                    ? (event.clientX - rect.left) / rect.width
                    : (event.clientY - rect.top) / rect.height;
                setTarget({
                  type: "groupSide",
                  id: group.id,
                  placement: ratio < 0.5 ? "before" : "after",
                });
              }}
              onDragLeave={(event) => {
                if (
                  !event.currentTarget.contains(
                    event.relatedTarget as Node | null,
                  ) &&
                  sideTarget
                )
                  setTarget(null);
              }}
              onDrop={(event) => {
                if (attached) return;
                if (dragItem?.type !== "group" || dragItem.id === group.id)
                  return;
                event.preventDefault();
                event.stopPropagation();
                dropGroupBesideGroup(dragItem.id, group.id, sideTarget ?? "after");
              }}
            >
              {/* An attached group has no label row of its own: its project's
                  tab is the header, and the submodules indent under it. */}
              {attached && parentRepo && renderRepoTab(parentRepo, group)}
              {!attached && (
              <TooltipTrigger asChild>
                <button
                  type="button"
                  draggable={!attached}
                  onDragStart={
                    attached
                      ? undefined
                      : (event) => startGroupDrag(event, group)
                  }
                  onDragEnd={
                    attached
                      ? undefined
                      : () => {
                          draggedGroupRef.current = group.id;
                          window.setTimeout(() => {
                            if (draggedGroupRef.current === group.id)
                              draggedGroupRef.current = null;
                          }, 200);
                          finishDrag();
                        }
                  }
                  onDragOver={(event) => {
                    if (attached || dragItem?.type !== "repo") return;
                    event.preventDefault();
                    event.stopPropagation();
                    setTarget({ type: "group", id: group.id });
                  }}
                  onDragLeave={(event) => {
                    if (
                      !event.currentTarget.contains(
                        event.relatedTarget as Node | null,
                      ) &&
                      groupTarget
                    )
                      setTarget(null);
                  }}
                  onDrop={(event) => {
                    if (attached) return;
                    event.preventDefault();
                    event.stopPropagation();
                    dropOnGroup(group);
                  }}
                  onClick={() => {
                    if (draggedGroupRef.current === group.id) {
                      draggedGroupRef.current = null;
                      return;
                    }
                    useWorkspaceStore.getState().toggleTabGroup(group.id);
                  }}
                  className={cn(
                    "gw-tab-group-handle flex flex-none items-center gap-1.5 text-left font-semibold outline-none",
                    attached
                      ? "cursor-pointer"
                      : "cursor-grab active:cursor-grabbing",
                    orientation === "horizontal"
                      ? "h-full min-w-8 px-2 text-2xs"
                      : effectiveIconOnly
                        ? "h-[29px] w-full justify-center rounded-[5px] px-0 text-2xs hover:bg-panel2"
                        : attached
                          ? // Sits one level in from its project's tab.
                            "h-[29px] w-full rounded-[5px] px-1.5 pl-5 text-2xs hover:bg-panel2"
                          : "h-[29px] w-full rounded-[5px] px-1.5 text-2xs hover:bg-panel2",
                    groupTarget &&
                      "bg-soft shadow-[inset_0_0_0_1px_var(--gw-accent)]",
                  )}
                  style={{ color: group.color }}
                  aria-label={
                    attached
                      ? `Parts of ${parentLabel}. ${group.collapsed ? "Expand" : "Collapse"}`
                      : `${group.name}. ${group.collapsed ? "Expand" : "Collapse"} group`
                  }
                >
                  <ChevronRight
                    size={11}
                    strokeWidth={2.2}
                    className={cn(
                      "flex-none transition-transform",
                      !group.collapsed && "rotate-90",
                    )}
                  />
                  {!(effectiveIconOnly && orientation === "vertical") && (
                    <>
                      <span className="max-w-28 overflow-hidden text-ellipsis whitespace-nowrap text-foreground">
                        {attached ? `Parts of ${parentLabel}` : group.name}
                      </span>
                      {saved && !attached && (
                        <Save size={10} strokeWidth={2} aria-label="Saved group" />
                      )}
                      <span className="font-mono text-2xs opacity-65">
                        {group.repoPaths.length}
                      </span>
                    </>
                  )}
                  {/* Everything pending inside, rolled up. Matters most while
                      the group is collapsed and its tabs are out of sight. */}
                  <GroupStatusIcons
                    group={group}
                    collapsed={effectiveIconOnly && orientation === "vertical"}
                  />
                </button>
              </TooltipTrigger>
              )}
              {!group.collapsed && (
                <div
                  className={cn(
                    "flex",
                    orientation === "horizontal"
                      ? "h-full flex-none flex-row"
                      : "w-full flex-col gap-0.5",
                    // Indentation for submodule rows lives on the tab itself
                    // (pl-9 above), so the container adds none of its own.
                  )}
                >
                  {nestedPaths.map((path) => {
                    const repo = findRepo(openRepos, path);
                    return repo ? renderRepoTab(repo, group) : null;
                  })}
                </div>
              )}
            </section>
          </ContextMenuTrigger>
          <ContextMenuContent className="w-52">
            <ContextMenuLabel className="text-2xs tracking-wide text-muted-foreground">
              {attached
                ? `PARTS OF ${parentLabel.toUpperCase()}`
                : `${group.name.toUpperCase()} · ${group.repoPaths.length} REPOS`}
            </ContextMenuLabel>
            <ContextMenuItem
              onSelect={() => {
                useWorkspaceStore.getState().toggleTabGroup(group.id);
              }}
            >
              <ChevronRight
                size={13}
                className={cn(!group.collapsed && "rotate-90")}
              />
              {group.collapsed ? "Expand" : "Collapse"}
            </ContextMenuItem>
            {/* Renaming, recolouring, saving and ungrouping all describe a
                group the user assembled. This one describes the folders. */}
            {attached ? (
              <>
                <ContextMenuSeparator />
                <ContextMenuItem
                  onSelect={() => {
                    if (parentRepo)
                      useWorkspaceStore.getState().setActiveRepo(parentRepo.id);
                  }}
                  disabled={!parentRepo}
                >
                  <Layers3 size={13} strokeWidth={2} />
                  Go to {parentLabel}
                </ContextMenuItem>
              </>
            ) : (
              <>
            <ContextMenuItem
              onSelect={() =>
                setRenaming({ type: "group", id: group.id, value: group.name })
              }
            >
              <Pencil size={13} strokeWidth={2} />
              Rename group
            </ContextMenuItem>
            <ContextMenuSub>
              <ContextMenuSubTrigger>
                <span
                  className="size-2.5 rounded-full"
                  style={{ background: group.color }}
                />
                Change color
              </ContextMenuSubTrigger>
              <ContextMenuSubContent className="w-40">
                {TAB_GROUP_COLORS.map((color) => (
                  <ContextMenuItem
                    key={color}
                    onSelect={() => {
                      useWorkspaceStore
                        .getState()
                        .setTabGroupColor(group.id, color);
                      toast.success(`${group.name} color changed`);
                    }}
                  >
                    <span
                      className="size-3 rounded-full"
                      style={{ background: color }}
                    />
                    <span className="flex-1">{color.toUpperCase()}</span>
                    {group.color === color && <Check size={12} />}
                  </ContextMenuItem>
                ))}
              </ContextMenuSubContent>
            </ContextMenuSub>
            <ContextMenuItem
              onSelect={() => {
                useWorkspaceStore.getState().saveTabGroup(group.id);
                toast.success(`${group.name} saved for later`);
              }}
            >
              <Save size={13} strokeWidth={2} />
              {saved ? "Update saved group" : "Save group"}
            </ContextMenuItem>
            <ContextMenuSeparator />
            <ContextMenuItem
              onSelect={() => {
                useWorkspaceStore.getState().ungroupTabGroup(group.id);
                toast.success(
                  `${group.name} ungrouped. Its repositories stayed in place.`,
                );
              }}
            >
              <Ungroup size={13} strokeWidth={2} />
              Ungroup
            </ContextMenuItem>
            <ContextMenuItem
              variant="destructive"
              onSelect={() => closeGroup(group)}
            >
              <Trash2 size={13} strokeWidth={2} />
              Close group
            </ContextMenuItem>
              </>
            )}
          </ContextMenuContent>
        </ContextMenu>
        <TooltipContent
          side={orientation === "vertical" ? "right" : "bottom"}
          className="w-80 max-w-[calc(100vw-16px)] p-0"
        >
          <div
            className="border-b border-border px-3 py-2.5"
            style={{ borderTop: `2px solid ${group.color}` }}
          >
            <div className="flex items-center justify-between gap-3">
              <span className="truncate text-xs font-semibold text-foreground">
                {attached ? `Parts of ${parentLabel}` : group.name}
              </span>
              <span className="flex-none font-mono text-[10px] text-muted-foreground">
                {group.repoPaths.length}{" "}
                {attached
                  ? group.repoPaths.length === 1
                    ? "part"
                    : "parts"
                  : group.repoPaths.length === 1
                    ? "repository"
                    : "repositories"}
              </span>
            </div>
          </div>
          <div className="grid gap-px bg-border">
            {groupRepos.map(({ path, repo }) => {
              const name = repo
                ? repoName(repo)
                : (path.split(/[\\/]/).filter(Boolean).at(-1) ?? path);
              return (
                <div
                  key={path}
                  className="flex min-w-0 gap-2.5 bg-panel3 px-3 py-2.5"
                >
                  <span
                    className="mt-0.5 grid size-6 flex-none place-items-center rounded-[5px] text-[10px] font-bold uppercase text-background"
                    style={{ background: group.color }}
                    aria-hidden="true"
                  >
                    {name.trim().charAt(0) || "R"}
                  </span>
                  <div className="min-w-0 flex-1">
                    <div className="flex items-center gap-2">
                      <span className="truncate text-xs font-semibold text-foreground">
                        {name}
                      </span>
                      <span className="ml-auto flex min-w-0 flex-none items-center gap-1 font-mono text-[10px] text-sub">
                        <GitBranch size={10} strokeWidth={2} />
                        <span className="max-w-28 truncate">
                          {repo?.head_branch ?? "No branch"}
                        </span>
                      </span>
                    </div>
                    <div className="mt-0.5 break-all font-mono text-[10px] leading-4 text-muted-foreground">
                      {path}
                    </div>
                  </div>
                </div>
              );
            })}
          </div>
        </TooltipContent>
      </Tooltip>
    );
  };

  /** Where a displayed item sits in tabOrder, which is what drops address. */
  const orderIndexOf = (item: TabOrderItem) =>
    tabOrder.findIndex((candidate) => sameOrderItem(item, candidate));

  /**
   * The gap in front of `anchor` -- or the trailing gap when it is null. Under
   * Name or Has-changes a gap does not map to a spot in the stored order, but
   * dropping into one adopts the displayed arrangement first and moves within
   * that, so the gaps stay live under every sort. `index` only identifies the
   * gap for highlighting; the drop itself is resolved from the anchor.
   */
  const renderOrderGap = (index: number, anchor: TabOrderItem | null) => {
    if (index < 0) return null;
    const active = dropTarget?.type === "order" && dropTarget.index === index;
    return (
      <div
        key={`gap-${index}`}
        data-tab-order-gap={index}
        onDragOver={(event) => {
          if (!dragItem) return;
          event.preventDefault();
          event.stopPropagation();
          setTarget({ type: "order", index });
        }}
        onDragLeave={(event) => {
          if (
            !event.currentTarget.contains(event.relatedTarget as Node | null) &&
            active
          )
            setTarget(null);
        }}
        onDrop={(event) => {
          event.preventDefault();
          event.stopPropagation();
          dropOnOrder(anchor);
        }}
        className={cn(
          "grid flex-none place-items-center overflow-hidden rounded-[5px] border-dashed text-2xs font-semibold text-accent-text transition-[width,height,margin,border-color,background-color] duration-150",
          orientation === "horizontal"
            ? active
              ? "mx-0.5 h-full w-24 border border-primary/60 bg-soft"
              : dragItem
                ? "h-full w-1.5"
                : index === 0
                  ? "h-full w-0"
                  : "h-full w-0.5"
            : active
              ? "my-0.5 h-8 w-full border border-primary/60 bg-soft"
              : dragItem
                ? "h-1.5 w-full"
                : "h-0.5 w-full",
        )}
      >
        {active
          ? dragItem?.type === "group"
            ? "Move group here"
            : "Move here"
          : ""}
      </div>
    );
  };

  return (
    <>
      {openRepos.map((repo) => (
        <ChangeCountReporter key={`count-${pathKey(repo.path)}`} repo={repo} />
      ))}
      {orientation === "horizontal" && (
        <ScrollArrow
          side="left"
          show={edges.start}
          onClick={() => scrollBy(-1)}
        />
      )}
      <div
        ref={scrollRef}
        data-dim-on-drag
        data-dragging-group={dragItem?.type === "group" ? "true" : undefined}
        className={cn(
          "gw-repository-tabs flex min-h-0 min-w-0",
          orientation === "horizontal"
            ? "gw-tab-scroll h-full flex-1 flex-row items-stretch overflow-x-auto overflow-y-hidden"
            : "w-full flex-1 flex-col overflow-y-auto overflow-x-hidden px-1.5 py-1",
        )}
        onDragEnd={finishDrag}
      >
        {pinnedOrder.length > 0 && (
          <div
            data-pinned-tabs
            className={cn(
              "flex flex-none",
              orientation === "horizontal"
                ? // Pinned tabs stay put while the rest of the strip scrolls under them.
                  "sticky left-0 z-20 h-full flex-row items-stretch border-r border-border bg-background"
                : "w-full flex-col gap-0.5 border-b border-border pb-1",
            )}
          >
            {pinnedOrder.map((item) =>
              item.type === "repo"
                ? (() => {
                    const repo = findRepo(openRepos, item.path);
                    return repo ? (
                      <Fragment key={`pinned-${pathKey(item.path)}`}>
                        {renderRepoTab(repo, null, true)}
                      </Fragment>
                    ) : null;
                  })()
                : null,
            )}
          </div>
        )}
        {displayOrder.map((item) => (
          <Fragment
            key={
              item.type === "group"
                ? `group-${item.id}`
                : `repo-${pathKey(item.path)}`
            }
          >
            {renderOrderGap(orderIndexOf(item), item)}
            {item.type === "group"
              ? (() => {
                  const group = tabGroups.find(
                    (candidate) => candidate.id === item.id,
                  );
                  return group ? renderGroup(group) : null;
                })()
              : (() => {
                  const repo = findRepo(openRepos, item.path);
                  return repo ? renderRepoTab(repo, null) : null;
                })()}
          </Fragment>
        ))}
        {renderOrderGap(tabOrder.length, null)}
        <AddRepoTab
          orientation={orientation}
          iconOnly={effectiveIconOnly}
          width={adaptiveHorizontalTabWidth}
        />
        {/* The horizontal strip stretches across the app bar so tab widths can
            be budgeted against the full width. Without this filler the leftover
            space belongs to the scroll container, which is not a drag region,
            and the window stops responding to drags and double-clicks there. */}
        {orientation === "horizontal" && (
          <div
            data-tauri-drag-region
            onDoubleClick={onTitleBarDoubleClick}
            className="w-0 flex-1 basis-0"
          />
        )}
      </div>
      {orientation === "horizontal" && (
        <ScrollArrow
          side="right"
          show={edges.end}
          onClick={() => scrollBy(1)}
        />
      )}

      {renaming && (
        <RenameDialog
          key={`${renaming.type}-${renaming.id}`}
          target={renaming}
          onClose={() => {
            if (renaming.type === "group" && renaming.isNewGroup) {
              useWorkspaceStore.getState().ungroupTabGroup(renaming.id);
              toast.success("Group cancelled, tabs left ungrouped");
            }
            setRenaming(null);
          }}
          onSave={(value) => {
            if (renaming.type === "tab") {
              useWorkspaceStore.getState().setTabAlias(renaming.id, value);
              const repo = findRepo(openRepos, renaming.id);
              toast.success(
                value.trim()
                  ? `Tab renamed to ${value.trim()}`
                  : `Tab name reset to ${repo?.name ?? "folder name"}`,
              );
            } else {
              useWorkspaceStore.getState().renameTabGroup(renaming.id, value);
              toast.success(
                renaming.isNewGroup
                  ? `Created the group ${value.trim()}`
                  : `Group renamed to ${value.trim()}`,
              );
            }
            setRenaming(null);
          }}
        />
      )}
      {iconRepo && (
        <RepoIconDialog
          key={iconRepo.path}
          repo={iconRepo}
          open
          onOpenChange={(open) => {
            if (!open) setIconRepo(null);
          }}
        />
      )}
    </>
  );
}
