import {
  Fragment,
  useCallback,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
  type CSSProperties,
  type DragEvent,
} from "react";
import {
  Check,
  ChevronLeft,
  ChevronRight,
  GitBranch,
  ImageIcon,
  Layers3,
  Pencil,
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
import { normalizePath } from "@/lib/paths";
import { useRepositoryPreview } from "@/lib/repositoryPreviews";
import {
  TAB_GROUP_COLORS,
  useWorkspaceStore,
  type TabDropPlacement,
  type TabGroup,
  type TabOrderItem,
} from "@/stores/workspaceStore";
import { useUiStore } from "@/stores/uiStore";
import { Button } from "@/components/ui/button";
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
  TooltipTrigger,
} from "@/components/ui/tooltip";
import { RepoIconDialog } from "@/components/domain/RepoIconDialog";

export type TabOrientation = "horizontal" | "vertical";

const MAX_HORIZONTAL_TAB_WIDTH = 208;
const ICON_ONLY_TAB_WIDTH = 38;
const VERTICAL_ICON_ONLY_WIDTH = 72;

type DragItem = { type: "repo"; path: string } | { type: "group"; id: string };

type DropTarget =
  | { type: "order"; index: number }
  | { type: "repo"; path: string; placement: TabDropPlacement | "group" }
  | { type: "group"; id: string };

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
  const preview = useRepositoryPreview(repo.id);

  return (
    <TooltipContent
      side={orientation === "vertical" ? "right" : "bottom"}
      className="w-80 max-w-[calc(100vw-16px)] overflow-hidden p-0"
    >
      {preview && (
        <div
          className="overflow-hidden border-b border-border bg-background"
          style={{ aspectRatio: `${preview.width} / ${preview.height}` }}
        >
          <img
            src={preview.dataUrl}
            alt=""
            draggable={false}
            className="size-full object-cover object-top"
          />
        </div>
      )}
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
  const savedTabGroups = useWorkspaceStore((state) => state.savedTabGroups);
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
  const verticalIconOnly =
    orientation === "vertical" && verticalTabWidth <= VERTICAL_ICON_ONLY_WIDTH;
  const effectiveIconOnly = tabIconOnly || verticalIconOnly;
  const showTabIcons = showRepoIcons || effectiveIconOnly;

  const visibleRepoCount = tabOrder.reduce((count, item) => {
    if (item.type === "repo") return count + (findRepo(openRepos, item.path) ? 1 : 0);
    const group = tabGroups.find((candidate) => candidate.id === item.id);
    if (!group || group.collapsed) return count;
    return count + group.repoPaths.filter((path) => findRepo(openRepos, path)).length;
  }, 0);
  const groupHeaderBudget = tabOrder.reduce((width, item) => {
    if (item.type !== "group") return width;
    const group = tabGroups.find((candidate) => candidate.id === item.id);
    return group ? width + Math.min(128, 48 + group.name.length * 6) : width;
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
    void commands.closeRepo(repo.id);
    useWorkspaceStore.getState().removeRepo(repo.id);
    toast.success(`Closed ${repoName(repo)}`);
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

  const dropOnOrder = (index: number) => {
    if (!dragItem) return;
    if (dragItem.type === "group") {
      useWorkspaceStore.getState().moveGroupToOrder(dragItem.id, index);
      const group = tabGroups.find((candidate) => candidate.id === dragItem.id);
      toast.success(`${group?.name ?? "Group"} moved`);
    } else {
      useWorkspaceStore.getState().moveRepoToOrder(dragItem.path, index);
      const repo = findRepo(openRepos, dragItem.path);
      toast.success(
        `${repo ? repoName(repo) : "Repository"} moved between tabs`,
      );
    }
    finishDrag();
  };

  const dropOnRepo = (
    targetPath: string,
    placement: TabDropPlacement | "group",
  ) => {
    if (dragItem?.type !== "repo" || samePath(dragItem.path, targetPath))
      return;
    const store = useWorkspaceStore.getState();
    const sourceRepo = findRepo(openRepos, dragItem.path);
    const targetRepo = findRepo(openRepos, targetPath);
    const targetGroup = tabGroups.find((group) =>
      group.repoPaths.some((path) => samePath(path, targetPath)),
    );
    const sourceGroup = tabGroups.find((group) =>
      group.repoPaths.some((path) => samePath(path, dragItem.path)),
    );

    if (placement !== "group") {
      store.moveRepoBeside(dragItem.path, targetPath, placement);
      toast.success(
        `${sourceRepo ? repoName(sourceRepo) : "Repository"} moved ${placement} ${targetRepo ? repoName(targetRepo) : "the tab"}`,
      );
    } else if (targetGroup) {
      if (sourceGroup?.id === targetGroup.id) {
        toast.info(
          `${sourceRepo ? repoName(sourceRepo) : "Repository"} is already in ${targetGroup.name}`,
        );
      } else {
        store.addRepoToGroup(dragItem.path, targetGroup.id);
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

  const renderRepoTab = (repo: RepoInfo, group: TabGroup | null) => {
    const target =
      dropTarget?.type === "repo" && samePath(dropTarget.path, repo.path)
        ? dropTarget.placement
        : null;
    const inGroup = group != null;
    const active = repo.id === activeRepoId;
    const groupsForMenu = tabGroups.filter(
      (candidate) => candidate.id !== group?.id,
    );
    const pathOrder = orderedPaths(tabOrder, tabGroups);
    const pathIndex = pathOrder.findIndex((path) => samePath(path, repo.path));
    const hovered = hoveredRepoPath != null && samePath(hoveredRepoPath, repo.path);
    const showName = !effectiveIconOnly || (orientation === "horizontal" && hovered);
    const horizontalWidth = tabIconOnly && hovered
      ? MAX_HORIZONTAL_TAB_WIDTH
      : adaptiveHorizontalTabWidth;
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
              draggable
              onDragStart={(event) => startRepoDrag(event, repo)}
              onDragEnd={finishDrag}
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
                    : inGroup
                      ? "h-[31px] w-full flex-none rounded-[5px] border px-2 pl-7"
                      : "h-[31px] w-full flex-none rounded-[5px] border px-2 pl-2",
                effectiveIconOnly && orientation === "horizontal" && !hovered &&
                  "justify-center px-1",
                inGroup && orientation === "horizontal"
                  ? "border-[color:color-mix(in_srgb,var(--tab-group-color)_20%,var(--gw-border))]"
                  : "border-border",
                active
                  ? inGroup
                    ? "bg-[color:color-mix(in_srgb,var(--tab-group-color)_10%,var(--gw-panel))] font-semibold text-foreground"
                    : "bg-panel font-semibold text-foreground"
                  : "text-sub hover:bg-panel2 hover:text-foreground",
                target === "group" &&
                  "z-10 border-primary! bg-soft! shadow-[0_0_0_2px_var(--gw-accent-soft)]",
              )}
              style={tabStyle}
              aria-label={repoName(repo)}
            >
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
              <TooltipButton
                draggable={false}
                onClick={(event) => {
                  event.stopPropagation();
                  closeRepo(repo);
                }}
                className={cn(
                  "ml-auto flex flex-none items-center justify-center overflow-hidden rounded text-muted-foreground transition-[width,opacity,margin] duration-150 hover:bg-panel3 hover:text-foreground",
                  "w-0 opacity-0 group-hover/repo:w-[15px] group-hover/repo:opacity-100 group-focus-within/repo:w-[15px] group-focus-within/repo:opacity-100",
                  orientation === "horizontal" &&
                    "-ml-[7px] group-hover/repo:ml-0 group-focus-within/repo:ml-0",
                )}
                tooltip="Close repository"
              >
                <X size={11} />
              </TooltipButton>
                </div>
              </TooltipTrigger>
            </ContextMenuTrigger>
            <ContextMenuContent className="w-52">
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
    const saved = isSaved(group.id);
    const groupRepos = group.repoPaths.map((path) => ({
      path,
      repo: findRepo(openRepos, path),
    }));
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
              )}
              style={groupStyle(group.color)}
            >
              <TooltipTrigger asChild>
                <button
                  type="button"
                  draggable
                  onDragStart={(event) => startGroupDrag(event, group)}
                  onDragEnd={() => {
                    draggedGroupRef.current = group.id;
                    window.setTimeout(() => {
                      if (draggedGroupRef.current === group.id)
                        draggedGroupRef.current = null;
                    }, 200);
                    finishDrag();
                  }}
                  onDragOver={(event) => {
                    if (dragItem?.type !== "repo") return;
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
                    toast.info(
                      `${group.name} ${group.collapsed ? "expanded" : "collapsed"}`,
                    );
                  }}
                  className={cn(
                    "flex flex-none cursor-grab items-center gap-1.5 text-left font-semibold outline-none active:cursor-grabbing",
                    orientation === "horizontal"
                      ? "h-full min-w-8 px-2 text-2xs"
                      : effectiveIconOnly
                        ? "h-[29px] w-full justify-center rounded-[5px] px-0 text-2xs hover:bg-panel2"
                        : "h-[29px] w-full rounded-[5px] px-1.5 text-2xs hover:bg-panel2",
                    groupTarget &&
                      "bg-soft shadow-[inset_0_0_0_1px_var(--gw-accent)]",
                  )}
                  style={{ color: group.color }}
                  aria-label={`${group.name}. ${group.collapsed ? "Expand" : "Collapse"} group`}
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
                        {group.name}
                      </span>
                      {saved && (
                        <Save size={10} strokeWidth={2} aria-label="Saved group" />
                      )}
                      <span className="font-mono text-2xs opacity-65">
                        {group.repoPaths.length}
                      </span>
                    </>
                  )}
                </button>
              </TooltipTrigger>
              {!group.collapsed && (
                <div
                  className={cn(
                    "flex",
                    orientation === "horizontal"
                      ? "h-full flex-none flex-row"
                      : "w-full flex-col gap-0.5",
                  )}
                >
                  {group.repoPaths.map((path) => {
                    const repo = findRepo(openRepos, path);
                    return repo ? renderRepoTab(repo, group) : null;
                  })}
                </div>
              )}
            </section>
          </ContextMenuTrigger>
          <ContextMenuContent className="w-52">
            <ContextMenuLabel className="text-2xs tracking-wide text-muted-foreground">
              {group.name.toUpperCase()} · {group.repoPaths.length} REPOS
            </ContextMenuLabel>
            <ContextMenuItem
              onSelect={() => {
                useWorkspaceStore.getState().toggleTabGroup(group.id);
                toast.info(
                  `${group.name} ${group.collapsed ? "expanded" : "collapsed"}`,
                );
              }}
            >
              <ChevronRight
                size={13}
                className={cn(!group.collapsed && "rotate-90")}
              />
              {group.collapsed ? "Expand group" : "Collapse group"}
            </ContextMenuItem>
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
                {group.name}
              </span>
              <span className="flex-none font-mono text-[10px] text-muted-foreground">
                {group.repoPaths.length}{" "}
                {group.repoPaths.length === 1 ? "repository" : "repositories"}
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

  const renderOrderGap = (index: number) => {
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
          dropOnOrder(index);
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
        className={cn(
          "gw-repository-tabs flex min-h-0 min-w-0",
          orientation === "horizontal"
            ? "gw-tab-scroll h-full flex-1 flex-row items-stretch overflow-x-auto overflow-y-hidden"
            : "w-full flex-1 flex-col overflow-y-auto overflow-x-hidden px-1.5 py-1",
        )}
        onDragEnd={finishDrag}
      >
        {tabOrder.map((item, index) => (
          <Fragment
            key={
              item.type === "group"
                ? `group-${item.id}`
                : `repo-${pathKey(item.path)}`
            }
          >
            {renderOrderGap(index)}
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
        {renderOrderGap(tabOrder.length)}
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
