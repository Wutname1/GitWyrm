import { useEffect, useMemo, useRef, useState } from "react";
import {
  ArrowRight,
  Check,
  ChevronRight,
  Clock3,
  Code2,
  Download,
  Eye,
  Folder,
  FolderGit2,
  FolderPlus,
  FolderSearch,
  Layers3,
  Loader2,
  Pin,
  Plus,
  RefreshCw,
  Search,
  ShieldCheck,
  Star,
  Trash2,
  X,
} from "lucide-react";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { GithubIcon } from "@/components/domain/github/GithubIcon";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { TooltipButton } from "@/components/ui/tooltip";
import { useGithubAuth, useGithubRepositories } from "@/hooks/useGithub";
import {
  useCodeFolderRepos,
  useOpenRepo,
  useOpenRepos,
} from "@/hooks/useRepoActions";
import {
  commands,
  type GithubRepository,
  type RepositoryStarter,
} from "@/lib/bindings";
import { isTauri } from "@/lib/env";
import { joinPath, normalizePath } from "@/lib/paths";
import { unwrap } from "@/lib/queryKeys";
import { cn } from "@/lib/utils";
import { useUiStore } from "@/stores/uiStore";
import {
  useWorkspaceStore,
  type RecentRepo,
  type RepoPickerSection,
  type SavedTabGroup,
} from "@/stores/workspaceStore";

interface GitProgressPayload {
  operation: string;
  line: string;
}

type Route = "open" | "clone" | "new";
type ProjectPathStatus = "idle" | "checking" | "available" | "exists" | "error";

interface LibraryRepo {
  name: string;
  path: string;
  headBranch: string | null;
}

type SelectedItem =
  | { type: "repo"; path: string }
  | { type: "group"; id: string }
  | null;

const STARTERS: {
  id: RepositoryStarter;
  name: string;
  detail: string;
  included: string;
}[] = [
  {
    id: "blank",
    name: "Blank",
    detail: "Just the folder",
    included: "No .gitignore",
  },
  {
    id: "node",
    name: "Node",
    detail: "Ignores packages",
    included: ".gitignore for Node",
  },
  {
    id: "rust",
    name: "Rust",
    detail: "Ignores builds",
    included: ".gitignore for Rust",
  },
  {
    id: "csharp",
    name: "C#",
    detail: ".NET and Visual Studio",
    included: ".gitignore for .NET",
  },
  {
    id: "all_in_one",
    name: "All-in-one",
    detail: "Covers most stacks",
    included: "Broad .gitignore for common tools and languages",
  },
];

function pathKey(path: string): string {
  return normalizePath(path).toLowerCase();
}

function pathName(path: string): string {
  const parts = normalizePath(path).split("\\");
  return parts.at(-1) || path;
}

function formatActivity(value: string): string {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return "Recently updated";
  const days = Math.max(
    0,
    Math.floor((Date.now() - date.getTime()) / 86_400_000),
  );
  if (days === 0) return "Updated today";
  if (days === 1) return "Updated yesterday";
  if (days < 7) return `Updated ${days} days ago`;
  return `Updated ${date.toLocaleDateString(undefined, { month: "short", day: "numeric" })}`;
}

function SectionHeading({
  children,
  count,
  note,
  action,
  collapsed,
  onToggle,
}: {
  children: React.ReactNode;
  count?: number;
  note?: string;
  action?: React.ReactNode;
  collapsed?: boolean;
  onToggle?: () => void;
}) {
  const heading = (
    <>
      <h2 className="text-2xs font-bold uppercase tracking-[.09em] text-sub">
        {children}
      </h2>
      {count != null && (
        <span className="rounded-full bg-panel3 px-1.5 py-0.5 font-mono text-[9px] text-muted-foreground">
          {count}
        </span>
      )}
      {note && <span className="text-2xs text-muted-foreground">{note}</span>}
      <span className="flex-1" />
    </>
  );

  return (
    <div className="mb-2 flex min-h-7 items-center gap-2">
      {onToggle ? (
        <button
          type="button"
          aria-expanded={!collapsed}
          onClick={onToggle}
          className="-mx-1 flex min-h-7 min-w-0 flex-1 items-center gap-2 rounded px-1 text-left hover:bg-panel3"
        >
          {heading}
          <span className="text-2xs font-medium text-muted-foreground">
            {collapsed ? "Show" : "Hide"}
          </span>
        </button>
      ) : (
        <div className="flex min-w-0 flex-1 items-center gap-2">{heading}</div>
      )}
      {action}
    </div>
  );
}

function RouteButton({
  active,
  icon,
  title,
  detail,
  shortcut,
  onClick,
}: {
  active: boolean;
  icon: React.ReactNode;
  title: string;
  detail: string;
  shortcut: string;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      aria-pressed={active}
      onClick={onClick}
      className={cn(
        "group flex min-h-[58px] min-w-0 items-center gap-3 rounded-lg border px-2.5 py-2.5 text-left transition-colors",
        active
          ? "border-accent/40 bg-soft text-foreground"
          : "border-transparent text-sub hover:border-border hover:bg-panel3 hover:text-foreground",
      )}
    >
      <span
        className={cn(
          "grid size-9 flex-none place-items-center rounded-full border transition-colors",
          active
            ? "border-accent/50 bg-accent/10 text-accent-text shadow-[0_0_0_3px_color-mix(in_srgb,var(--gw-accent)_7%,transparent)]"
            : "border-border bg-panel text-muted-foreground group-hover:text-sub",
        )}
      >
        {icon}
      </span>
      <span className="min-w-0 flex-1">
        <strong className="block text-xs font-semibold leading-4">
          {title}
        </strong>
        <small className="mt-0.5 block text-2xs leading-4 text-muted-foreground">
          {detail}
        </small>
      </span>
      <kbd className="font-mono text-[9px] text-muted-foreground">
        {shortcut}
      </kbd>
    </button>
  );
}

function RepoLibraryRow({
  repo,
  pinned,
  openRepoId,
  selected,
  checked,
  busy,
  onSelect,
  onToggleChecked,
  onTogglePin,
  onOpen,
  onJump,
}: {
  repo: LibraryRepo;
  pinned: boolean;
  openRepoId?: string;
  selected: boolean;
  checked: boolean;
  busy: boolean;
  onSelect: () => void;
  onToggleChecked: () => void;
  onTogglePin: () => void;
  onOpen: () => void;
  onJump: () => void;
}) {
  return (
    <div
      className={cn(
        "group/repo grid min-h-12 grid-cols-[auto_minmax(0,1fr)_auto] items-center rounded-md border pr-1.5 transition-colors",
        selected
          ? "border-accent/35 bg-soft"
          : "border-transparent hover:border-border/70 hover:bg-panel",
      )}
    >
      <button
        type="button"
        aria-label={`${checked ? "Uncheck" : "Check"} ${repo.name}`}
        aria-pressed={checked}
        onClick={onToggleChecked}
        disabled={busy}
        className="ml-2.5 grid size-6 place-items-center rounded hover:bg-panel3 disabled:opacity-50"
      >
        <span
          className={cn(
            "grid size-4 place-items-center rounded border transition-colors",
            checked
              ? "border-accent bg-accent text-accent-foreground"
              : "border-border bg-background",
          )}
        >
          {checked && <Check size={11} strokeWidth={2.5} />}
        </span>
      </button>
      <button
        type="button"
        aria-pressed={selected}
        onClick={onSelect}
        onDoubleClick={openRepoId ? onJump : onOpen}
        disabled={busy}
        className="flex min-w-0 items-center gap-2.5 px-2.5 py-1.5 text-left disabled:opacity-50"
      >
        <span className="grid size-8 flex-none place-items-center rounded-md border border-border bg-panel3 text-sub">
          <FolderGit2 size={15} strokeWidth={1.8} />
        </span>
        <span className="min-w-0 flex-1">
          <span className="block truncate text-xs font-semibold text-foreground">
            {repo.name}
          </span>
          <span className="block truncate font-mono text-2xs text-muted-foreground">
            {repo.path}
          </span>
        </span>
        {repo.headBranch && (
          <span className="hidden flex-none font-mono text-2xs text-sub min-[1100px]:block">
            {repo.headBranch}
          </span>
        )}
        {openRepoId && (
          <span className="rounded-full bg-accent/10 px-1.5 py-0.5 text-[9px] font-semibold text-accent-text">
            Open
          </span>
        )}
      </button>
      <div className="flex items-center gap-0.5">
        <TooltipButton
          tooltip={pinned ? `Unpin ${repo.name}` : `Pin ${repo.name}`}
          aria-pressed={pinned}
          onClick={onTogglePin}
          className={cn(
            "grid size-7 place-items-center rounded-[5px] hover:bg-panel3",
            pinned
              ? "text-accent-text"
              : "text-muted-foreground opacity-0 group-hover/repo:opacity-100 focus:opacity-100",
          )}
        >
          <Pin size={12} fill={pinned ? "currentColor" : "none"} />
        </TooltipButton>
        <Button
          variant={selected ? "secondary" : "ghost"}
          size="sm"
          className="h-7 gap-1 px-2 text-2xs opacity-0 group-hover/repo:opacity-100 focus:opacity-100"
          onClick={openRepoId ? onJump : onOpen}
          disabled={busy}
        >
          {openRepoId ? "Jump" : "Open"}
          <ChevronRight size={11} />
        </Button>
      </div>
    </div>
  );
}

function SavedGroupCard({
  group,
  selected,
  opening,
  onSelect,
  onOpen,
}: {
  group: SavedTabGroup;
  selected: boolean;
  opening: boolean;
  onSelect: () => void;
  onOpen: () => void;
}) {
  const names = group.repoPaths.map(pathName);
  return (
    <div
      className={cn(
        "group/card grid min-w-0 rounded-lg border p-3 text-left transition-colors",
        selected
          ? "border-accent/40 bg-soft"
          : "border-border bg-panel hover:border-border-bright hover:bg-panel2",
      )}
    >
      <button
        type="button"
        className="min-w-0 text-left"
        onClick={onSelect}
        onDoubleClick={onOpen}
      >
        <span className="flex items-center gap-2">
          <span
            className="size-2.5 rounded-full"
            style={{
              background: group.color,
              boxShadow: `0 0 0 3px ${group.color}22`,
            }}
          />
          <strong className="truncate text-xs font-semibold text-foreground">
            {group.name}
          </strong>
        </span>
        <span className="mt-3 flex min-w-0 gap-1 overflow-hidden">
          {names.slice(0, 2).map((name) => (
            <span
              key={name}
              className="max-w-28 truncate rounded bg-background px-1.5 py-1 font-mono text-[9px] text-sub"
            >
              {name}
            </span>
          ))}
          {names.length > 2 && (
            <span className="rounded bg-background px-1.5 py-1 font-mono text-[9px] text-sub">
              +{names.length - 2}
            </span>
          )}
        </span>
      </button>
      <span className="mt-3 flex items-center gap-2 text-2xs text-muted-foreground">
        {group.repoPaths.length}{" "}
        {group.repoPaths.length === 1 ? "repository" : "repositories"}
        <span className="flex-1" />
        <Button
          variant="ghost"
          size="sm"
          className="h-6 gap-1 px-1.5 text-2xs opacity-0 group-hover/card:opacity-100 focus:opacity-100"
          onClick={onOpen}
          disabled={opening}
        >
          {opening ? <Loader2 size={11} className="animate-spin" /> : null}
          {opening ? "Opening" : "Open"}
        </Button>
      </span>
    </div>
  );
}

function GroupLibrary({
  open,
  onOpenChange,
  groups,
  pinnedIds,
  openingGroupId,
  onTogglePin,
  onOpenGroup,
  onDelete,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  groups: SavedTabGroup[];
  pinnedIds: string[];
  openingGroupId: string | null;
  onTogglePin: (id: string) => void;
  onOpenGroup: (id: string) => void;
  onDelete: (id: string) => void;
}) {
  const [filter, setFilter] = useState("");
  const filtered = useMemo(() => {
    const query = filter.trim().toLowerCase();
    if (!query) return groups;
    return groups.filter((group) =>
      `${group.name} ${group.repoPaths.join(" ")}`
        .toLowerCase()
        .includes(query),
    );
  }, [filter, groups]);

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="flex max-h-[min(760px,calc(100vh-64px))] flex-col gap-0 overflow-hidden p-0 sm:max-w-4xl">
        <DialogHeader className="border-b border-border px-5 py-4 pr-12">
          <div className="flex items-center gap-4">
            <div className="min-w-0 flex-1">
              <DialogTitle className="text-base">Saved groups</DialogTitle>
              <DialogDescription className="mt-1 text-xs">
                Pin the groups you use most. The three newest pins appear on the
                add screen.
              </DialogDescription>
            </div>
            <label className="relative w-64">
              <Search
                size={13}
                className="pointer-events-none absolute left-2.5 top-1/2 -translate-y-1/2 text-muted-foreground"
              />
              <Input
                value={filter}
                onChange={(event) => setFilter(event.target.value)}
                placeholder="Find a group"
                aria-label="Find a saved group"
                className="h-8 bg-background pl-8 text-xs"
              />
            </label>
          </div>
        </DialogHeader>
        <div className="grid min-h-0 flex-1 grid-cols-1 gap-2 overflow-y-auto p-3 sm:grid-cols-2">
          {filtered.map((group) => {
            const pinned = pinnedIds.includes(group.id);
            const opening = openingGroupId === group.id;
            return (
              <div
                key={group.id}
                className="group/library grid min-h-16 grid-cols-[8px_minmax(0,1fr)_auto] items-center gap-3 rounded-md border border-border bg-panel px-3 py-2"
              >
                <span
                  className="size-2 rounded-full"
                  style={{ background: group.color }}
                />
                <span className="min-w-0">
                  <strong className="block truncate text-xs text-foreground">
                    {group.name}
                  </strong>
                  <small className="mt-0.5 block truncate font-mono text-2xs text-muted-foreground">
                    {group.repoPaths.map(pathName).join(" · ")}
                  </small>
                </span>
                <span className="flex items-center gap-1">
                  <TooltipButton
                    tooltip={
                      pinned ? `Unpin ${group.name}` : `Pin ${group.name}`
                    }
                    aria-pressed={pinned}
                    onClick={() => onTogglePin(group.id)}
                    className={cn(
                      "grid size-7 place-items-center rounded-[5px] hover:bg-panel3",
                      pinned ? "text-accent-text" : "text-muted-foreground",
                    )}
                  >
                    <Pin size={12} fill={pinned ? "currentColor" : "none"} />
                  </TooltipButton>
                  <TooltipButton
                    tooltip={`Delete ${group.name}`}
                    onClick={() => onDelete(group.id)}
                    disabled={openingGroupId != null}
                    className="grid size-7 place-items-center rounded-[5px] text-muted-foreground opacity-0 hover:bg-panel3 hover:text-removed group-hover/library:opacity-100 focus:opacity-100"
                  >
                    <Trash2 size={12} />
                  </TooltipButton>
                  <Button
                    size="sm"
                    className="h-7 min-w-16 gap-1 text-2xs"
                    onClick={() => onOpenGroup(group.id)}
                    disabled={openingGroupId != null}
                  >
                    {opening ? (
                      <Loader2 size={11} className="animate-spin" />
                    ) : null}
                    {opening ? "Opening" : "Open"}
                  </Button>
                </span>
              </div>
            );
          })}
          {filtered.length === 0 && (
            <div className="col-span-full grid min-h-48 place-items-center rounded-md border border-dashed border-border text-center">
              <div>
                <Search
                  size={20}
                  className="mx-auto mb-2 text-muted-foreground"
                />
                <strong className="text-xs text-foreground">
                  No groups match that search
                </strong>
                <p className="mt-1 text-2xs text-muted-foreground">
                  Try a group or repository name.
                </p>
              </div>
            </div>
          )}
        </div>
      </DialogContent>
    </Dialog>
  );
}

function OnlineRepositoryCard({
  repository,
  onChoose,
}: {
  repository: GithubRepository;
  onChoose: () => void;
}) {
  return (
    <button
      type="button"
      onClick={onChoose}
      className="group grid w-full gap-1 rounded-md border border-border bg-panel px-3 py-2.5 text-left hover:border-border-bright hover:bg-panel2"
    >
      <span className="flex min-w-0 items-center gap-2">
        {repository.starred ? (
          <Star
            size={12}
            className="flex-none text-amber-400"
            fill="currentColor"
          />
        ) : (
          <GithubIcon size={12} className="flex-none text-sub" />
        )}
        <strong className="min-w-0 flex-1 truncate text-xs text-foreground">
          {repository.full_name}
        </strong>
        {repository.private && (
          <span className="rounded bg-panel3 px-1.5 py-0.5 text-[9px] text-muted-foreground">
            Private
          </span>
        )}
      </span>
      {repository.description && (
        <small className="line-clamp-2 text-2xs leading-4 text-muted-foreground">
          {repository.description}
        </small>
      )}
      <span className="mt-1 flex items-center gap-1 text-2xs text-sub">
        {formatActivity(repository.pushed_at)}
        <span className="flex-1" />
        Use this link
        <ArrowRight
          size={11}
          className="transition-transform group-hover:translate-x-0.5"
        />
      </span>
    </button>
  );
}

function RepoDetails({
  selected,
  repositories,
  groups,
  openByPath,
  pinnedRepoPaths,
  openingGroupId,
  busy,
  onToggleRepoPin,
  onOpenRepo,
  onJumpToRepo,
  onOpenGroup,
}: {
  selected: SelectedItem;
  repositories: LibraryRepo[];
  groups: SavedTabGroup[];
  openByPath: Map<string, { id: string }>;
  pinnedRepoPaths: string[];
  openingGroupId: string | null;
  busy: boolean;
  onToggleRepoPin: (path: string) => void;
  onOpenRepo: (path: string) => void;
  onJumpToRepo: (id: string) => void;
  onOpenGroup: (id: string) => void;
}) {
  if (selected?.type === "group") {
    const group = groups.find((candidate) => candidate.id === selected.id);
    if (!group) return null;
    return (
      <>
        <div className="border-b border-border px-5 py-4 text-2xs font-bold uppercase tracking-[.09em] text-muted-foreground">
          Group details
        </div>
        <div className="flex min-h-0 flex-1 flex-col overflow-y-auto p-5">
          <span
            className="grid size-10 place-items-center rounded-lg border border-border bg-panel3"
            style={{ color: group.color }}
          >
            <Layers3 size={20} />
          </span>
          <h2 className="mt-4 text-base font-semibold text-foreground">
            {group.name}
          </h2>
          <p className="mt-1 text-xs text-muted-foreground">
            Opens {group.repoPaths.length}{" "}
            {group.repoPaths.length === 1 ? "repository" : "repositories"}{" "}
            together.
          </p>
          <div className="mt-5 border-t border-border pt-3">
            {group.repoPaths.map((path) => (
              <div key={path} className="flex items-center gap-2 py-1.5">
                <FolderGit2 size={13} className="flex-none text-sub" />
                <span className="min-w-0">
                  <strong className="block truncate text-xs text-foreground">
                    {pathName(path)}
                  </strong>
                  <small className="block truncate font-mono text-[9px] text-muted-foreground">
                    {path}
                  </small>
                </span>
              </div>
            ))}
          </div>
          <span className="flex-1" />
          <Button
            className="mt-5 gap-2"
            onClick={() => onOpenGroup(group.id)}
            disabled={openingGroupId != null}
          >
            {openingGroupId === group.id ? (
              <Loader2 size={14} className="animate-spin" />
            ) : (
              <Layers3 size={14} />
            )}
            {openingGroupId === group.id ? "Opening group…" : "Open group"}
          </Button>
        </div>
      </>
    );
  }

  const repo =
    selected?.type === "repo"
      ? repositories.find(
          (candidate) => pathKey(candidate.path) === pathKey(selected.path),
        )
      : repositories[0];
  if (!repo) {
    return (
      <div className="grid flex-1 place-items-center p-6 text-center">
        <div>
          <FolderSearch size={22} className="mx-auto text-muted-foreground" />
          <p className="mt-2 text-xs text-muted-foreground">
            Choose a repository to see its details.
          </p>
        </div>
      </div>
    );
  }
  const openRepo = openByPath.get(pathKey(repo.path));
  const pinned = pinnedRepoPaths.some(
    (path) => pathKey(path) === pathKey(repo.path),
  );
  return (
    <>
      <div className="border-b border-border px-5 py-4 text-2xs font-bold uppercase tracking-[.09em] text-muted-foreground">
        Repository details
      </div>
      <div className="flex min-h-0 flex-1 flex-col overflow-y-auto p-5">
        <span className="grid size-10 place-items-center rounded-lg border border-accent/25 bg-accent/10 text-accent-text">
          <FolderGit2 size={20} />
        </span>
        <h2 className="mt-4 truncate text-base font-semibold text-foreground">
          {repo.name}
        </h2>
        <p className="mt-1 break-all font-mono text-2xs leading-4 text-muted-foreground">
          {repo.path}
        </p>
        <div className="mt-4 flex flex-wrap gap-1.5">
          {repo.headBranch && (
            <span className="rounded-full bg-panel3 px-2 py-1 font-mono text-2xs text-sub">
              {repo.headBranch}
            </span>
          )}
          {openRepo && (
            <span className="rounded-full bg-accent/10 px-2 py-1 text-2xs font-semibold text-accent-text">
              Open now
            </span>
          )}
          {pinned && (
            <span className="rounded-full bg-panel3 px-2 py-1 text-2xs text-sub">
              Pinned
            </span>
          )}
        </div>
        <div className="mt-5 border-t border-border pt-3">
          <button
            type="button"
            className="flex w-full items-center gap-2 rounded-md px-2 py-2 text-left text-xs text-sub hover:bg-panel3 hover:text-foreground"
            onClick={() => onToggleRepoPin(repo.path)}
          >
            <Pin size={13} fill={pinned ? "currentColor" : "none"} />
            {pinned ? "Remove from pinned repositories" : "Pin this repository"}
          </button>
        </div>
        <span className="flex-1" />
        <Button
          className="mt-5 gap-2"
          onClick={() =>
            openRepo ? onJumpToRepo(openRepo.id) : onOpenRepo(repo.path)
          }
          disabled={busy}
        >
          {busy ? (
            <Loader2 size={14} className="animate-spin" />
          ) : (
            <FolderGit2 size={14} />
          )}
          {openRepo ? `Open ${repo.name} tab` : `Open ${repo.name}`}
        </Button>
      </div>
    </>
  );
}

function RepoPickerPanel({
  onDone,
  wiggleNonce = 0,
}: {
  onDone: () => void;
  wiggleNonce?: number;
}) {
  const recents = useWorkspaceStore((state) => state.recents);
  const openRepos = useWorkspaceStore((state) => state.openRepos);
  const codeFolder = useWorkspaceStore((state) => state.codeFolder);
  const cloneDirectory = useWorkspaceStore((state) => state.cloneDirectory);
  const savedTabGroups = useWorkspaceStore((state) => state.savedTabGroups);
  const pinnedRepoPaths = useWorkspaceStore((state) => state.pinnedRepoPaths);
  const pinnedSavedGroupIds = useWorkspaceStore(
    (state) => state.pinnedSavedGroupIds,
  );
  const repoPickerCollapsedSections = useWorkspaceStore(
    (state) => state.repoPickerCollapsedSections,
  );
  const setCodeFolder = useWorkspaceStore((state) => state.setCodeFolder);
  const setCloneDirectory = useWorkspaceStore(
    (state) => state.setCloneDirectory,
  );
  const addRepo = useWorkspaceStore((state) => state.addRepo);
  const createSavedTabGroup = useWorkspaceStore(
    (state) => state.createSavedTabGroup,
  );
  const deleteSavedTabGroup = useWorkspaceStore(
    (state) => state.deleteSavedTabGroup,
  );
  const togglePinnedRepo = useWorkspaceStore((state) => state.togglePinnedRepo);
  const togglePinnedSavedGroup = useWorkspaceStore(
    (state) => state.togglePinnedSavedGroup,
  );
  const toggleRepoPickerSection = useWorkspaceStore(
    (state) => state.toggleRepoPickerSection,
  );
  const openModal = useUiStore((state) => state.openModal);

  const scanned = useCodeFolderRepos();
  const openRepo = useOpenRepo();
  const openReposMutation = useOpenRepos();
  const githubAuth = useGithubAuth();
  const githubRepositories = useGithubRepositories(githubAuth.data != null);

  const [route, setRoute] = useState<Route>("open");
  const [filter, setFilter] = useState("");
  const [selectedItem, setSelectedItem] = useState<SelectedItem>(null);
  const [selectedPaths, setSelectedPaths] = useState<Set<string>>(new Set());
  const [namingGroup, setNamingGroup] = useState(false);
  const [groupName, setGroupName] = useState("");
  const [groupLibraryOpen, setGroupLibraryOpen] = useState(false);
  const [openingGroupId, setOpeningGroupId] = useState<string | null>(null);
  const [dropHover, setDropHover] = useState(false);
  const [dropStatus, setDropStatus] = useState("");
  const [wiggling, setWiggling] = useState(false);

  const defaultDestination = codeFolder ?? cloneDirectory ?? "";
  const [url, setUrl] = useState("");
  const [cloneDestination, setCloneDestination] = useState(defaultDestination);
  const [cloneFolderName, setCloneFolderName] = useState("");
  const [cloneNameTouched, setCloneNameTouched] = useState(false);
  const [cloning, setCloning] = useState(false);
  const [cloneProgress, setCloneProgress] = useState("");
  const [onlineFilter, setOnlineFilter] = useState("");

  const [newDestination, setNewDestination] = useState(defaultDestination);
  const [newName, setNewName] = useState("");
  const [starter, setStarter] = useState<RepositoryStarter>("blank");
  const [addReadme, setAddReadme] = useState(true);
  const [createInitialCommit, setCreateInitialCommit] = useState(true);
  const [initializing, setInitializing] = useState(false);
  const [projectPathStatus, setProjectPathStatus] =
    useState<ProjectPathStatus>("idle");

  const searchRef = useRef<HTMLInputElement>(null);
  const busy = openRepo.isPending || openReposMutation.isPending;
  const isSectionCollapsed = (section: RepoPickerSection) =>
    repoPickerCollapsedSections.includes(section);
  const toggleLibrarySection = (
    section: RepoPickerSection,
    label: string,
  ) => {
    const collapsed = isSectionCollapsed(section);
    toggleRepoPickerSection(section);
    toast(`${label} ${collapsed ? "shown" : "hidden"}`);
  };
  const openByPath = useMemo(
    () => new Map(openRepos.map((repo) => [pathKey(repo.path), repo])),
    [openRepos],
  );

  const libraryRepos = useMemo(() => {
    const byPath = new Map<string, LibraryRepo>();
    for (const repo of scanned.data ?? []) {
      byPath.set(pathKey(repo.path), {
        name: repo.name,
        path: repo.path,
        headBranch: repo.head_branch,
      });
    }
    for (const repo of recents) {
      const key = pathKey(repo.path);
      if (!byPath.has(key)) {
        byPath.set(key, { name: repo.name, path: repo.path, headBranch: null });
      }
    }
    for (const path of pinnedRepoPaths) {
      const key = pathKey(path);
      if (!byPath.has(key)) {
        byPath.set(key, { name: pathName(path), path, headBranch: null });
      }
    }
    return [...byPath.values()];
  }, [pinnedRepoPaths, recents, scanned.data]);

  const repoByPath = useMemo(
    () => new Map(libraryRepos.map((repo) => [pathKey(repo.path), repo])),
    [libraryRepos],
  );
  const query = filter.trim().toLowerCase();
  const matches = (repo: LibraryRepo) =>
    !query ||
    `${repo.name} ${repo.path} ${repo.headBranch ?? ""}`
      .toLowerCase()
      .includes(query);
  const pinnedRepos = pinnedRepoPaths.flatMap((path) => {
    const repo = repoByPath.get(pathKey(path));
    return repo && matches(repo) ? [repo] : [];
  });

  const recentRepos = recents
    .filter((repo) => !openByPath.has(pathKey(repo.path)))
    .filter(
      (repo) =>
        !pinnedRepoPaths.some((path) => pathKey(path) === pathKey(repo.path)),
    )
    .slice(0, 5)
    .flatMap((repo): LibraryRepo[] => {
      const item = repoByPath.get(pathKey(repo.path));
      return item && matches(item) ? [item] : [];
    });

  const hiddenKeys = new Set(
    [...pinnedRepos, ...recentRepos].map((repo) => pathKey(repo.path)),
  );
  const otherRepos = (scanned.data ?? [])
    .flatMap((repo): LibraryRepo[] => {
      const item = repoByPath.get(pathKey(repo.path));
      return item ? [item] : [];
    })
    .filter((repo) => !hiddenKeys.has(pathKey(repo.path)) && matches(repo));

  const pinnedGroups = pinnedSavedGroupIds
    .flatMap((id) => {
      const group = savedTabGroups.find((candidate) => candidate.id === id);
      if (!group) return [];
      const searchable =
        `${group.name} ${group.repoPaths.join(" ")}`.toLowerCase();
      return !query || searchable.includes(query) ? [group] : [];
    })
    .slice(0, 3);

  const jumpToRepo = (repoId: string) => {
    useWorkspaceStore.getState().setActiveRepo(repoId);
    onDone();
  };

  const openOne = (path: string) => {
    const existing = openByPath.get(pathKey(path));
    if (existing) {
      jumpToRepo(existing.id);
      return;
    }
    setDropStatus(`Opening ${pathName(path)}…`);
    openRepo.mutate(path, {
      onSuccess: () => {
        setDropStatus("");
        onDone();
      },
      onError: () => setDropStatus(""),
    });
  };

  const openMany = (paths: string[]) => {
    if (paths.length === 0) return;
    setDropStatus(`Opening ${paths.length} folders…`);
    openReposMutation.mutate(paths, {
      onSuccess: () => {
        setDropStatus("");
        onDone();
      },
      onError: () => setDropStatus(""),
    });
  };

  useEffect(() => {
    if (wiggleNonce > 0) setWiggling(true);
  }, [wiggleNonce]);

  useEffect(() => {
    setCloneDestination(defaultDestination);
    setNewDestination(defaultDestination);
  }, [defaultDestination]);

  useEffect(() => {
    if (!cloning) return;
    const unlisten = listen<GitProgressPayload>("git-progress", (event) => {
      if (event.payload.operation === "clone")
        setCloneProgress(event.payload.line);
    });
    return () => {
      unlisten.then((stop) => stop());
    };
  }, [cloning]);

  useEffect(() => {
    if (!isTauri || route !== "open") return;
    let active = true;
    let stop: (() => void) | undefined;
    getCurrentWebview()
      .onDragDropEvent((event) => {
        if (!active) return;
        if (event.payload.type === "enter" || event.payload.type === "over") {
          setDropHover(true);
        } else if (event.payload.type === "leave") {
          setDropHover(false);
        } else {
          setDropHover(false);
          openMany(event.payload.paths);
        }
      })
      .then((unlisten) => {
        if (active) stop = unlisten;
        else unlisten();
      });
    return () => {
      active = false;
      stop?.();
    };
  }, [route]);

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      const target = event.target as HTMLElement | null;
      if (target?.closest('input, textarea, [contenteditable="true"]')) return;
      if (event.key === "/") {
        event.preventDefault();
        setRoute("open");
        requestAnimationFrame(() => searchRef.current?.focus());
      } else if (event.key === "1") {
        setRoute("open");
      } else if (event.key === "2") {
        setRoute("clone");
      } else if (event.key === "3") {
        setRoute("new");
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, []);

  useEffect(() => {
    if (route !== "open" || selectedItem != null) return;
    const firstGroup = pinnedGroups[0];
    const firstRepo = pinnedRepos[0] ?? recentRepos[0] ?? otherRepos[0];
    if (firstGroup) setSelectedItem({ type: "group", id: firstGroup.id });
    else if (firstRepo) setSelectedItem({ type: "repo", path: firstRepo.path });
  }, [otherRepos, pinnedGroups, pinnedRepos, recentRepos, route, selectedItem]);

  const pickCodeFolder = async () => {
    const { open } = await import("@tauri-apps/plugin-dialog");
    const directory = await open({
      directory: true,
      title: "Choose your watched folder",
    });
    if (typeof directory !== "string") return;
    setCodeFolder(directory);
    toast.success("Watched folder changed");
  };

  const browseForRepositories = async () => {
    const { open } = await import("@tauri-apps/plugin-dialog");
    const directory = await open({
      directory: true,
      multiple: true,
      title: "Open repositories",
    });
    const paths = Array.isArray(directory)
      ? directory
      : typeof directory === "string"
        ? [directory]
        : [];
    if (paths.length === 1) openOne(paths[0]);
    else openMany(paths);
  };

  const toggleSelectedPath = (path: string) => {
    const key = pathKey(path);
    setSelectedPaths((current) => {
      const next = new Set(current);
      if (next.has(key)) next.delete(key);
      else next.add(key);
      return next;
    });
  };

  const selectedOriginalPaths = useMemo(
    () =>
      [...selectedPaths].flatMap((key) => {
        const repo = repoByPath.get(key);
        return repo ? [repo.path] : [];
      }),
    [repoByPath, selectedPaths],
  );

  const clearSelection = () => {
    setSelectedPaths(new Set());
    setNamingGroup(false);
    setGroupName("");
  };

  const saveSelectedGroup = () => {
    const id = createSavedTabGroup(selectedOriginalPaths, groupName);
    if (!id) return;
    const name = groupName.trim();
    clearSelection();
    setSelectedItem({ type: "group", id });
    toast.success(`${name} saved and pinned`);
  };

  const openSelectedRepositories = () => {
    const paths = selectedOriginalPaths.filter(
      (path) => !openByPath.has(pathKey(path)),
    );
    if (paths.length === 0) {
      toast("Those repositories are already open");
      return;
    }
    clearSelection();
    openMany(paths);
  };

  const toggleRepoPin = (repo: LibraryRepo) => {
    const pinned = pinnedRepoPaths.some(
      (path) => pathKey(path) === pathKey(repo.path),
    );
    togglePinnedRepo(repo.path);
    toast.success(pinned ? `${repo.name} unpinned` : `${repo.name} pinned`);
  };

  const toggleGroupPin = (groupId: string) => {
    const group = savedTabGroups.find((candidate) => candidate.id === groupId);
    if (!group) return;
    const pinned = pinnedSavedGroupIds.includes(groupId);
    togglePinnedSavedGroup(groupId);
    toast.success(pinned ? `${group.name} unpinned` : `${group.name} pinned`);
  };

  const openSavedGroup = async (groupId: string) => {
    const group = savedTabGroups.find((candidate) => candidate.id === groupId);
    if (!group || openingGroupId) return;
    setOpeningGroupId(groupId);
    const toastId = toast.loading(`Opening ${group.name}…`);
    const openedPaths: string[] = [];
    const failures: string[] = [];
    try {
      for (const path of group.repoPaths) {
        const alreadyOpen = useWorkspaceStore
          .getState()
          .openRepos.find((repo) => pathKey(repo.path) === pathKey(path));
        if (alreadyOpen) {
          openedPaths.push(alreadyOpen.path);
          continue;
        }
        try {
          const repo = unwrap(await commands.openRepo(path));
          addRepo(repo);
          openedPaths.push(repo.path);
        } catch {
          failures.push(path);
        }
      }
      if (openedPaths.length > 0) {
        useWorkspaceStore.getState().createTabGroup(openedPaths, {
          id: group.id,
          name: group.name,
          color: group.color,
        });
        onDone();
      }
      if (failures.length === 0) {
        toast.success(`Opened ${group.name}`);
      } else if (openedPaths.length > 0) {
        toast.warning(
          `${failures.length} ${failures.length === 1 ? "repository was" : "repositories were"} unavailable`,
        );
      } else {
        toast.error(
          `None of the repositories in ${group.name} could be opened`,
        );
      }
    } finally {
      toast.dismiss(toastId);
      setOpeningGroupId(null);
      setGroupLibraryOpen(false);
    }
  };

  const suggestedCloneName =
    url
      .trim()
      .replace(/\/+$/, "")
      .split("/")
      .at(-1)
      ?.replace(/\.git$/, "") ?? "";

  useEffect(() => {
    if (!cloneNameTouched) setCloneFolderName(suggestedCloneName);
  }, [cloneNameTouched, suggestedCloneName]);

  const finalCloneName = cloneFolderName.trim() || suggestedCloneName;
  const clonePath =
    finalCloneName && cloneDestination.trim()
      ? joinPath(normalizePath(cloneDestination), finalCloneName)
      : "";

  const chooseOnlineRepository = (repository: GithubRepository) => {
    setUrl(repository.clone_url);
    setCloneFolderName(repository.full_name.split("/").at(-1) ?? "");
    setCloneNameTouched(false);
    toast.success(`${repository.full_name} selected`);
  };

  const doClone = async () => {
    if (!url.trim() || !finalCloneName || !cloneDestination.trim()) return;
    const destinationRoot = normalizePath(cloneDestination);
    const destination = joinPath(destinationRoot, finalCloneName);
    setCloneDirectory(destinationRoot);
    setCloning(true);
    setCloneProgress("Starting copy…");
    try {
      const cloned = unwrap(await commands.gitClone(url.trim(), destination));
      const repo = unwrap(await commands.openRepo(cloned));
      addRepo(repo);
      toast.success(`Copied ${repo.name}`);
      setUrl("");
      setCloneFolderName("");
      setCloneNameTouched(false);
      onDone();
    } catch (error) {
      toast.error((error as Error).message);
    } finally {
      setCloning(false);
      setCloneProgress("");
    }
  };

  const newFolder = newName.trim()
    ? joinPath(normalizePath(newDestination), newName.trim())
    : "";
  const selectedStarter =
    STARTERS.find((candidate) => candidate.id === starter) ?? STARTERS[0];

  useEffect(() => {
    if (!newFolder || !newDestination.trim()) {
      setProjectPathStatus("idle");
      return;
    }
    if (!isTauri) {
      setProjectPathStatus("available");
      return;
    }

    let active = true;
    setProjectPathStatus("checking");
    const timer = window.setTimeout(() => {
      void commands
        .pathExists(newFolder)
        .then((exists) => {
          if (active) setProjectPathStatus(exists ? "exists" : "available");
        })
        .catch(() => {
          if (active) setProjectPathStatus("error");
        });
    }, 160);

    return () => {
      active = false;
      window.clearTimeout(timer);
    };
  }, [newDestination, newFolder]);

  const doInit = async () => {
    if (
      !newFolder ||
      !newDestination.trim() ||
      projectPathStatus !== "available"
    )
      return;
    setCloneDirectory(normalizePath(newDestination));
    setInitializing(true);
    try {
      if (isTauri && (await commands.pathExists(newFolder))) {
        setProjectPathStatus("exists");
        toast.error("This project already exists");
        return;
      }
      const created = unwrap(
        await commands.gitInit(
          newFolder,
          starter,
          addReadme,
          createInitialCommit,
        ),
      );
      const repo = unwrap(await commands.openRepo(created));
      addRepo(repo);
      toast.success(`Created ${repo.name}`);
      setNewName("");
      onDone();
    } catch (error) {
      toast.error((error as Error).message);
    } finally {
      setInitializing(false);
    }
  };

  const renderRepoRow = (repo: LibraryRepo) => {
    const key = pathKey(repo.path);
    const open = openByPath.get(key);
    return (
      <RepoLibraryRow
        key={key}
        repo={repo}
        pinned={pinnedRepoPaths.some((path) => pathKey(path) === key)}
        openRepoId={open?.id}
        selected={
          selectedItem?.type === "repo" && pathKey(selectedItem.path) === key
        }
        checked={selectedPaths.has(key)}
        busy={busy}
        onSelect={() => setSelectedItem({ type: "repo", path: repo.path })}
        onToggleChecked={() => toggleSelectedPath(repo.path)}
        onTogglePin={() => toggleRepoPin(repo)}
        onOpen={() => openOne(repo.path)}
        onJump={() => open && jumpToRepo(open.id)}
      />
    );
  };

  const onlineQuery = onlineFilter.trim().toLowerCase();
  const linkedRepositories = (githubRepositories.data ?? []).filter((repo) => {
    if (!onlineQuery) return true;
    return `${repo.full_name} ${repo.description ?? ""} ${repo.html_url}`
      .toLowerCase()
      .includes(onlineQuery);
  });
  const starredRepositories = linkedRepositories.filter((repo) => repo.starred);
  const accessibleRepositories = linkedRepositories.filter(
    (repo) => !repo.starred,
  );

  const openScreen = (
    <>
      <div className="border-b border-border px-5 py-4 min-[1200px]:px-7 min-[1200px]:py-5">
        <p className="text-[9px] font-bold uppercase tracking-[.12em] text-accent-text">
          On this computer
        </p>
        <h1 className="mt-1 text-xl font-semibold tracking-tight text-foreground">
          What are you working on?
        </h1>
        <p className="mt-1 text-xs text-muted-foreground">
          Open one repository, launch a saved group, or browse to another
          folder.
        </p>
        <div className="mt-4 flex gap-2">
          <label className="relative min-w-0 flex-1">
            <Search
              size={14}
              className="pointer-events-none absolute left-3 top-1/2 -translate-y-1/2 text-muted-foreground"
            />
            <Input
              ref={searchRef}
              value={filter}
              onChange={(event) => setFilter(event.target.value)}
              placeholder="Find a repository or group"
              aria-label="Find a repository or group"
              className="h-9 bg-panel pl-9 pr-8 text-xs"
            />
            <kbd className="pointer-events-none absolute right-3 top-1/2 -translate-y-1/2 font-mono text-[9px] text-muted-foreground">
              /
            </kbd>
          </label>
          {savedTabGroups.length > 0 && (
            <Button
              variant="secondary"
              size="sm"
              className="h-9 flex-none gap-1.5 text-xs"
              onClick={() => setGroupLibraryOpen(true)}
            >
              <Layers3 size={13} />
              Groups
            </Button>
          )}
          <Button
            variant="secondary"
            size="sm"
            className="h-9 flex-none gap-1.5 text-xs"
            onClick={browseForRepositories}
            disabled={busy}
          >
            <FolderSearch size={13} />
            Browse…
          </Button>
        </div>
      </div>

      <button
        type="button"
        data-repo-drop-zone
        onClick={browseForRepositories}
        className={cn(
          "mx-5 mt-3 flex min-h-9 items-center gap-2 rounded-md border border-dashed px-3 text-left text-2xs transition-colors min-[1200px]:mx-7",
          dropHover
            ? "border-accent bg-accent/10 text-accent-text"
            : "border-border text-muted-foreground hover:border-border-bright hover:bg-panel",
        )}
      >
        {busy || dropStatus ? (
          <Loader2 size={13} className="animate-spin" />
        ) : (
          <Download size={13} />
        )}
        <span>
          <strong className="font-semibold text-sub">
            {dropStatus ||
              (dropHover ? "Drop to open now" : "Drop a project folder here")}
          </strong>
          {!dropStatus && !dropHover ? " to open it right away" : ""}
        </span>
        <span className="flex-1" />
        <span>Folders only</span>
      </button>

      <div
        className={cn(
          "min-h-0 flex-1 overflow-y-auto px-5 pt-5 min-[1200px]:px-7",
          selectedOriginalPaths.length > 0 ? "pb-24" : "pb-6",
        )}
      >
        <div className="mx-auto grid w-full max-w-[1600px] gap-6 min-[2200px]:grid-cols-[minmax(0,1fr)_390px]">
          {pinnedGroups.length > 0 && (
            <section className="min-w-0 min-[2200px]:col-start-2 min-[2200px]:row-start-1">
              <SectionHeading
                count={pinnedSavedGroupIds.length}
                note={pinnedSavedGroupIds.length > 3 ? "3 shown" : undefined}
                collapsed={isSectionCollapsed("pinned_groups")}
                onToggle={() =>
                  toggleLibrarySection("pinned_groups", "Pinned groups")
                }
                action={
                  <Button
                    variant="ghost"
                    size="sm"
                    className="h-7 text-2xs"
                    onClick={() => setGroupLibraryOpen(true)}
                  >
                    View all {savedTabGroups.length}
                  </Button>
                }
              >
                Pinned groups
              </SectionHeading>
              {!isSectionCollapsed("pinned_groups") && (
                <div className="grid gap-2 min-[1100px]:grid-cols-3 min-[2200px]:grid-cols-1">
                  {pinnedGroups.map((group) => (
                    <SavedGroupCard
                      key={group.id}
                      group={group}
                      selected={
                        selectedItem?.type === "group" &&
                        selectedItem.id === group.id
                      }
                      opening={openingGroupId === group.id}
                      onSelect={() =>
                        setSelectedItem({ type: "group", id: group.id })
                      }
                      onOpen={() => void openSavedGroup(group.id)}
                    />
                  ))}
                </div>
              )}
            </section>
          )}

          {pinnedRepos.length > 0 && (
            <section className="min-w-0 min-[2200px]:col-start-1 min-[2200px]:row-start-1">
              <SectionHeading
                count={pinnedRepos.length}
                collapsed={isSectionCollapsed("pinned_repositories")}
                onToggle={() =>
                  toggleLibrarySection(
                    "pinned_repositories",
                    "Pinned repositories",
                  )
                }
              >
                Pinned repositories
              </SectionHeading>
              {!isSectionCollapsed("pinned_repositories") && (
                <div className="grid gap-0.5">
                  {pinnedRepos.map(renderRepoRow)}
                </div>
              )}
            </section>
          )}

          <section
            className={cn(
              "min-w-0",
              (pinnedGroups.length > 0 || pinnedRepos.length > 0) && "mt-6",
              "min-[2200px]:col-start-2 min-[2200px]:mt-0",
              pinnedGroups.length > 0 || pinnedRepos.length > 0
                ? "min-[2200px]:row-start-2"
                : "min-[2200px]:row-start-1",
            )}
          >
            <SectionHeading
              count={recentRepos.length}
              note="not open now"
              collapsed={isSectionCollapsed("recent")}
              onToggle={() => toggleLibrarySection("recent", "Recent repositories")}
            >
              Recent
            </SectionHeading>
            {!isSectionCollapsed("recent") && (
              <div className="grid gap-0.5">
                {recentRepos.map(renderRepoRow)}
              </div>
            )}
          </section>

          <section
            className={cn(
              "min-w-0 mt-6 min-[2200px]:col-start-1 min-[2200px]:mt-0",
              pinnedGroups.length > 0 || pinnedRepos.length > 0
                ? "min-[2200px]:row-start-2"
                : "min-[2200px]:row-start-1",
            )}
          >
            <SectionHeading
              count={otherRepos.length}
              note={codeFolder ? `in ${codeFolder}` : undefined}
              collapsed={isSectionCollapsed("watched")}
              onToggle={() =>
                toggleLibrarySection(
                  "watched",
                  codeFolder ? "Watched folder" : "Repositories",
                )
              }
              action={
                codeFolder ? (
                  <TooltipButton
                    tooltip="Rescan watched folder"
                    onClick={() => {
                      scanned.refetch();
                      toast("Rescanning watched folder…");
                    }}
                    className="grid size-7 place-items-center rounded-[5px] text-muted-foreground hover:bg-panel3 hover:text-foreground"
                  >
                    <RefreshCw
                      size={12}
                      className={cn(scanned.isFetching && "animate-spin")}
                    />
                  </TooltipButton>
                ) : undefined
              }
            >
              {codeFolder ? "Watched folder" : "Repositories"}
            </SectionHeading>
            {!isSectionCollapsed("watched") && (
              <>
                {!codeFolder && (
                  <button
                    type="button"
                    onClick={pickCodeFolder}
                    className="grid w-full place-items-center rounded-md border border-dashed border-border px-5 py-10 text-center hover:border-border-bright hover:bg-panel"
                  >
                    <Eye size={20} className="text-muted-foreground" />
                    <strong className="mt-2 text-xs text-foreground">
                      Choose a folder to watch
                    </strong>
                    <span className="mt-1 text-2xs text-muted-foreground">
                      GitWyrm will find the repositories inside it.
                    </span>
                  </button>
                )}
                {codeFolder && scanned.isLoading && (
                  <div className="flex items-center gap-2 px-3 py-6 text-xs text-muted-foreground">
                    <Loader2 size={14} className="animate-spin" />
                    Finding repositories…
                  </div>
                )}
                {codeFolder && scanned.isError && (
                  <div className="rounded-md border border-removed/30 bg-removed/5 px-3 py-3 text-xs text-removed">
                    {(scanned.error as Error).message}
                  </div>
                )}
                <div className="grid gap-0.5">
                  {otherRepos.map(renderRepoRow)}
                </div>
              </>
            )}
          </section>
        </div>
      </div>

      {selectedOriginalPaths.length > 0 && (
        <div className="absolute inset-x-0 bottom-0 z-20 border-t border-border bg-panel/95 px-5 py-3 shadow-[0_-10px_30px_rgba(0,0,0,.24)] backdrop-blur min-[1200px]:px-7">
          <div className="mx-auto flex max-w-[1600px] items-center gap-2">
            {namingGroup ? (
              <>
                <Input
                  value={groupName}
                  onChange={(event) => setGroupName(event.target.value)}
                  placeholder="Group name"
                  aria-label="Group name"
                  className="h-8 max-w-sm bg-background text-xs"
                  autoFocus
                />
                <Button
                  variant="secondary"
                  size="sm"
                  className="h-8 text-xs"
                  onClick={() => setNamingGroup(false)}
                >
                  Back
                </Button>
                <Button
                  size="sm"
                  className="h-8 gap-1.5 text-xs"
                  onClick={saveSelectedGroup}
                  disabled={
                    !groupName.trim() || selectedOriginalPaths.length === 0
                  }
                >
                  <Layers3 size={13} />
                  Save and pin group
                </Button>
              </>
            ) : (
              <>
                <span className="text-xs text-muted-foreground">
                  {selectedOriginalPaths.length} checked
                </span>
                <span className="flex-1" />
                <Button
                  variant="ghost"
                  size="sm"
                  className="h-8 text-xs"
                  onClick={clearSelection}
                >
                  Clear
                </Button>
                <Button
                  variant="secondary"
                  size="sm"
                  className="h-8 gap-1.5 text-xs"
                  onClick={() => setNamingGroup(true)}
                >
                  <Layers3 size={13} />
                  Create group
                </Button>
                <Button
                  size="sm"
                  className="h-8 gap-1.5 text-xs"
                  onClick={openSelectedRepositories}
                  disabled={busy}
                >
                  {busy && <Loader2 size={13} className="animate-spin" />}
                  Open selected
                </Button>
              </>
            )}
          </div>
        </div>
      )}
    </>
  );

  const cloneScreen = (
    <>
      <div className="border-b border-border px-5 py-5 min-[1200px]:px-7">
        <p className="text-[9px] font-bold uppercase tracking-[.12em] text-accent-text">
          From a web address
        </p>
        <h1 className="mt-1 text-xl font-semibold tracking-tight text-foreground">
          Copy a repository to this computer
        </h1>
        <p className="mt-1 text-xs text-muted-foreground">
          Paste a link from GitHub, GitLab, Bitbucket, or any Git server.
        </p>
      </div>
      <div className="min-h-0 flex-1 overflow-y-auto p-5 min-[1200px]:p-7">
        <div className="mx-auto grid w-full max-w-[1120px] gap-5 min-[1100px]:grid-cols-[minmax(420px,680px)_minmax(280px,360px)]">
          <form
            className="self-start rounded-lg border border-border bg-panel"
            onSubmit={(event) => {
              event.preventDefault();
              void doClone();
            }}
          >
            <div className="border-b border-border px-5 py-4">
              <h2 className="text-sm font-semibold text-foreground">
                Repository link
              </h2>
              <p className="mt-1 text-2xs text-muted-foreground">
                GitWyrm uses your normal Git sign-in if the project is private.
              </p>
            </div>
            <div className="grid gap-4 p-5">
              <label className="grid gap-1.5 text-xs font-semibold text-sub">
                Web address
                <Input
                  value={url}
                  onChange={(event) => setUrl(event.target.value)}
                  placeholder="https://github.com/team/project.git"
                  className="h-9 bg-background font-mono text-xs font-normal"
                  disabled={cloning}
                  autoFocus
                />
              </label>
              <label className="grid gap-1.5 text-xs font-semibold text-sub">
                Save in
                <span className="flex gap-2">
                  <Input
                    value={cloneDestination}
                    onChange={(event) =>
                      setCloneDestination(event.target.value)
                    }
                    onBlur={() =>
                      cloneDestination.trim() &&
                      setCloneDestination(normalizePath(cloneDestination))
                    }
                    placeholder="Choose a parent folder"
                    className="h-9 bg-background font-mono text-xs font-normal"
                    disabled={cloning}
                  />
                  <Button
                    type="button"
                    variant="secondary"
                    size="sm"
                    className="h-9 gap-1.5"
                    onClick={async () => {
                      const { open } =
                        await import("@tauri-apps/plugin-dialog");
                      const directory = await open({
                        directory: true,
                        title: "Copy repository into…",
                      });
                      if (typeof directory === "string")
                        setCloneDestination(normalizePath(directory));
                    }}
                    disabled={cloning}
                  >
                    <Folder size={13} />
                    Browse…
                  </Button>
                </span>
              </label>
              <label className="grid gap-1.5 text-xs font-semibold text-sub">
                Folder name
                <Input
                  value={cloneFolderName}
                  onChange={(event) => {
                    setCloneFolderName(event.target.value);
                    setCloneNameTouched(true);
                  }}
                  placeholder={suggestedCloneName || "project"}
                  className="h-9 bg-background font-mono text-xs font-normal"
                  disabled={cloning}
                />
              </label>
              {clonePath && (
                <div className="rounded-md border border-border bg-background px-3 py-2.5">
                  <small className="text-2xs font-semibold text-muted-foreground">
                    Will be saved to
                  </small>
                  <code className="mt-1 block break-all text-xs text-foreground">
                    {clonePath}
                  </code>
                </div>
              )}
              <div className="flex min-h-8 items-center gap-2 border-t border-border pt-4">
                <span className="min-w-0 flex-1 truncate font-mono text-2xs text-muted-foreground">
                  {cloning
                    ? cloneProgress
                    : url.trim()
                      ? "Ready to copy"
                      : "Waiting for a repository link"}
                </span>
                <Button
                  type="submit"
                  size="sm"
                  className="h-8 min-w-32 gap-1.5"
                  disabled={
                    !url.trim() ||
                    !finalCloneName ||
                    !cloneDestination.trim() ||
                    cloning
                  }
                >
                  {cloning ? (
                    <Loader2 size={13} className="animate-spin" />
                  ) : (
                    <Download size={13} />
                  )}
                  {cloning ? "Copying…" : "Copy repository"}
                </Button>
              </div>
            </div>
          </form>

          <section className="min-w-0">
            {githubAuth.isLoading ? (
              <div className="flex items-center gap-2 rounded-lg border border-border bg-panel p-4 text-xs text-muted-foreground">
                <Loader2 size={14} className="animate-spin" />
                Checking GitHub…
              </div>
            ) : githubAuth.data == null ? (
              <div className="rounded-lg border border-border bg-panel p-4">
                <div className="flex items-center gap-2 text-xs font-semibold text-foreground">
                  <GithubIcon size={15} />
                  GitHub is not connected
                </div>
                <p className="mt-2 text-2xs leading-4 text-muted-foreground">
                  Connect to choose from your repositories and stars. You can
                  still paste any link.
                </p>
                <Button
                  variant="secondary"
                  size="sm"
                  className="mt-3 h-8 gap-1.5 text-xs"
                  onClick={() => openModal("githubConnect")}
                >
                  <GithubIcon size={13} />
                  Connect GitHub
                </Button>
              </div>
            ) : (
              <>
                <div className="rounded-lg border border-accent/25 bg-accent/5 p-4">
                  <div className="flex items-center gap-2 text-xs font-semibold text-foreground">
                    <GithubIcon size={14} />
                    GitHub connected
                    <span className="flex-1" />
                    <ShieldCheck size={13} className="text-accent-text" />
                  </div>
                  <p className="mt-1 text-2xs text-muted-foreground">
                    Signed in as {githubAuth.data}. GitWyrm only shows
                    repositories you can access.
                  </p>
                </div>
                <label className="relative mt-3 block">
                  <Search
                    size={14}
                    className="pointer-events-none absolute left-3 top-1/2 -translate-y-1/2 text-muted-foreground"
                  />
                  <Input
                    value={onlineFilter}
                    onChange={(event) => setOnlineFilter(event.target.value)}
                    placeholder="Search linked repositories"
                    aria-label="Search linked repositories"
                    className="h-9 bg-panel pl-9 pr-12 text-xs"
                  />
                  {onlineQuery && (
                    <span className="pointer-events-none absolute right-3 top-1/2 -translate-y-1/2 text-2xs text-muted-foreground">
                      {linkedRepositories.length}
                    </span>
                  )}
                </label>
                {githubRepositories.isLoading && (
                  <div className="mt-3 flex items-center gap-2 px-2 py-5 text-xs text-muted-foreground">
                    <Loader2 size={14} className="animate-spin" />
                    Loading repositories…
                  </div>
                )}
                {githubRepositories.isError && (
                  <div className="mt-3 rounded-md border border-removed/30 bg-removed/5 px-3 py-3 text-xs text-removed">
                    {(githubRepositories.error as Error).message}
                  </div>
                )}
                {!githubRepositories.isLoading &&
                  !githubRepositories.isError &&
                  onlineQuery &&
                  linkedRepositories.length === 0 && (
                    <div className="mt-3 rounded-md border border-dashed border-border px-4 py-8 text-center">
                      <Search
                        size={18}
                        className="mx-auto text-muted-foreground"
                      />
                      <p className="mt-2 text-xs font-semibold text-foreground">
                        No linked repositories found
                      </p>
                      <p className="mt-1 text-2xs text-muted-foreground">
                        Try a repository name, owner, or part of its web
                        address.
                      </p>
                    </div>
                  )}
                {starredRepositories.length > 0 && (
                  <div className="mt-5">
                    <SectionHeading
                      count={starredRepositories.length}
                      note="shown first"
                    >
                      Starred online
                    </SectionHeading>
                    <div className="grid gap-2">
                      {starredRepositories.slice(0, 6).map((repository) => (
                        <OnlineRepositoryCard
                          key={repository.full_name}
                          repository={repository}
                          onChoose={() => chooseOnlineRepository(repository)}
                        />
                      ))}
                    </div>
                  </div>
                )}
                {accessibleRepositories.length > 0 && (
                  <div className="mt-5">
                    <SectionHeading
                      count={accessibleRepositories.length}
                      note="recent activity"
                    >
                      Your GitHub repositories
                    </SectionHeading>
                    <div className="grid gap-2">
                      {accessibleRepositories.slice(0, 10).map((repository) => (
                        <OnlineRepositoryCard
                          key={repository.full_name}
                          repository={repository}
                          onChoose={() => chooseOnlineRepository(repository)}
                        />
                      ))}
                    </div>
                  </div>
                )}
              </>
            )}
          </section>
        </div>
      </div>
    </>
  );

  const newScreen = (
    <>
      <div className="border-b border-border px-5 py-5 min-[1200px]:px-7">
        <p className="text-[9px] font-bold uppercase tracking-[.12em] text-accent-text">
          New project
        </p>
        <h1 className="mt-1 text-xl font-semibold tracking-tight text-foreground">
          Start a new repository
        </h1>
        <p className="mt-1 text-xs text-muted-foreground">
          Create a folder that is ready for your first change. Nothing is sent
          online.
        </p>
      </div>
      <div className="min-h-0 flex-1 overflow-y-auto p-5 min-[1200px]:p-7">
        <div className="mx-auto grid w-full max-w-[1120px] gap-5 min-[1100px]:grid-cols-[minmax(520px,720px)_minmax(260px,340px)]">
          <form
            className={cn(
              "self-start rounded-lg border bg-panel transition-[border-color,box-shadow]",
              projectPathStatus === "exists" ||
                projectPathStatus === "error"
                ? "border-removed/80 shadow-[0_0_0_1px_color-mix(in_srgb,var(--gw-removed)_30%,transparent)]"
                : projectPathStatus === "available"
                  ? "border-accent/65 shadow-[0_0_0_1px_color-mix(in_srgb,var(--gw-accent)_20%,transparent)]"
                  : "border-border",
            )}
            onSubmit={(event) => {
              event.preventDefault();
              void doInit();
            }}
          >
            <div className="border-b border-border px-5 py-4">
              <h2 className="text-sm font-semibold text-foreground">
                Project basics
              </h2>
              <p className="mt-1 text-2xs text-muted-foreground">
                You can change these starter files later.
              </p>
            </div>
            <div className="grid gap-4 p-5">
              <label className="grid gap-1.5 text-xs font-semibold text-sub">
                <span className="flex min-h-4 items-center gap-2">
                  Project name
                  <span className="flex-1" />
                  {projectPathStatus === "checking" && (
                    <span className="flex items-center gap-1 text-2xs font-medium text-muted-foreground">
                      <Loader2 size={12} className="animate-spin" />
                      Checking…
                    </span>
                  )}
                  {projectPathStatus === "exists" && (
                    <span className="flex items-center gap-1 text-2xs font-semibold text-removed">
                      <X size={13} strokeWidth={2.6} />
                      This project exists
                    </span>
                  )}
                  {projectPathStatus === "error" && (
                    <span className="flex items-center gap-1 text-2xs font-semibold text-removed">
                      <X size={13} strokeWidth={2.6} />
                      Could not check this folder
                    </span>
                  )}
                  {projectPathStatus === "available" && (
                    <span className="flex items-center gap-1 text-2xs font-semibold text-accent-text">
                      <Check size={13} strokeWidth={2.6} />
                      Available
                    </span>
                  )}
                </span>
                <Input
                  value={newName}
                  onChange={(event) => setNewName(event.target.value)}
                  placeholder="my-project"
                  className={cn(
                    "h-9 bg-background font-mono text-xs font-normal",
                    (projectPathStatus === "exists" ||
                      projectPathStatus === "error") &&
                      "border-removed/70 focus-visible:ring-removed/35",
                    projectPathStatus === "available" &&
                      "border-accent/60 focus-visible:ring-accent/30",
                  )}
                  disabled={initializing}
                  autoFocus
                />
              </label>
              <label className="grid gap-1.5 text-xs font-semibold text-sub">
                Create in
                <span className="flex gap-2">
                  <Input
                    value={newDestination}
                    onChange={(event) => setNewDestination(event.target.value)}
                    onBlur={() =>
                      newDestination.trim() &&
                      setNewDestination(normalizePath(newDestination))
                    }
                    placeholder="Choose a parent folder"
                    className="h-9 bg-background font-mono text-xs font-normal"
                    disabled={initializing}
                  />
                  <Button
                    type="button"
                    variant="secondary"
                    size="sm"
                    className="h-9 gap-1.5"
                    onClick={async () => {
                      const { open } =
                        await import("@tauri-apps/plugin-dialog");
                      const directory = await open({
                        directory: true,
                        title: "Create repository in…",
                      });
                      if (typeof directory === "string")
                        setNewDestination(normalizePath(directory));
                    }}
                    disabled={initializing}
                  >
                    <Folder size={13} />
                    Browse…
                  </Button>
                </span>
              </label>
              <fieldset className="grid gap-2">
                <legend className="mb-1 text-xs font-semibold text-sub">
                  Starter
                </legend>
                <div className="grid grid-cols-2 gap-2 min-[1250px]:grid-cols-5">
                  {STARTERS.map((candidate) => (
                    <button
                      key={candidate.id}
                      type="button"
                      aria-pressed={starter === candidate.id}
                      onClick={() => {
                        setStarter(candidate.id);
                        toast(`${candidate.name} starter selected`);
                      }}
                      className={cn(
                        "grid min-h-16 content-center rounded-md border px-3 py-2 text-left",
                        starter === candidate.id
                          ? "border-accent/45 bg-soft text-foreground"
                          : "border-border bg-background text-sub hover:border-border-bright hover:bg-panel3",
                      )}
                    >
                      <strong className="text-xs">{candidate.name}</strong>
                      <small className="mt-1 text-[9px] leading-3 text-muted-foreground">
                        {candidate.detail}
                      </small>
                    </button>
                  ))}
                </div>
              </fieldset>
              <label className="flex cursor-pointer items-start gap-3 rounded-md border border-border bg-background px-3 py-2.5 hover:bg-panel3">
                <input
                  type="checkbox"
                  checked={addReadme}
                  onChange={(event) => setAddReadme(event.target.checked)}
                  className="mt-0.5 size-3.5 accent-[var(--gw-accent)]"
                />
                <span>
                  <strong className="block text-xs text-foreground">
                    Add a README
                  </strong>
                  <small className="mt-0.5 block text-2xs text-muted-foreground">
                    A simple front page with the project name.
                  </small>
                </span>
              </label>
              <label className="flex cursor-pointer items-start gap-3 rounded-md border border-border bg-background px-3 py-2.5 hover:bg-panel3">
                <input
                  type="checkbox"
                  checked={createInitialCommit}
                  onChange={(event) =>
                    setCreateInitialCommit(event.target.checked)
                  }
                  className="mt-0.5 size-3.5 accent-[var(--gw-accent)]"
                />
                <span>
                  <strong className="block text-xs text-foreground">
                    Save the starter files to history
                  </strong>
                  <small className="mt-0.5 block text-2xs text-muted-foreground">
                    Gives the project a clean starting point.
                  </small>
                </span>
              </label>
              {newFolder && (
                <div
                  className={cn(
                    "rounded-md border bg-background px-3 py-2.5",
                    projectPathStatus === "exists" ||
                      projectPathStatus === "error"
                      ? "border-removed/45"
                      : projectPathStatus === "available"
                        ? "border-accent/40"
                        : "border-border",
                  )}
                >
                  <small className="text-2xs font-semibold text-muted-foreground">
                    Will create
                  </small>
                  <code className="mt-1 block break-all text-xs text-foreground">
                    {newFolder}
                  </code>
                </div>
              )}
              <div className="flex min-h-8 items-center gap-2 border-t border-border pt-4">
                <span className="min-w-0 flex-1 text-2xs text-muted-foreground">
                  {projectPathStatus === "exists"
                    ? "Choose another project name"
                    : projectPathStatus === "error"
                      ? "Folder check failed"
                      : projectPathStatus === "checking"
                        ? "Checking the project folder…"
                        : projectPathStatus === "available"
                          ? "Ready to create"
                          : "Enter a project name"}
                </span>
                <Button
                  type="submit"
                  size="sm"
                  className="h-8 min-w-32 gap-1.5"
                  disabled={
                    !newFolder ||
                    !newDestination.trim() ||
                    projectPathStatus !== "available" ||
                    initializing
                  }
                >
                  {initializing ? (
                    <Loader2 size={13} className="animate-spin" />
                  ) : (
                    <FolderPlus size={13} />
                  )}
                  {initializing ? "Creating…" : "Create project"}
                </Button>
              </div>
            </div>
          </form>

          <section>
            <SectionHeading>Included</SectionHeading>
            <div className="rounded-lg border border-border bg-panel p-4">
              <span className="grid size-9 place-items-center rounded-md border border-accent/25 bg-accent/10 text-accent-text">
                <Code2 size={17} />
              </span>
              <h2 className="mt-3 text-sm font-semibold text-foreground">
                {selectedStarter.name} repository
              </h2>
              <ul className="mt-3 grid gap-2 text-2xs text-muted-foreground">
                {starter !== "blank" && (
                  <li className="flex items-start gap-2">
                    <Check
                      size={12}
                      className="mt-0.5 flex-none text-accent-text"
                    />
                    {selectedStarter.included}
                  </li>
                )}
                {addReadme && (
                  <li className="flex items-start gap-2">
                    <Check
                      size={12}
                      className="mt-0.5 flex-none text-accent-text"
                    />
                    README.md
                  </li>
                )}
                {createInitialCommit && (
                  <li className="flex items-start gap-2">
                    <Check
                      size={12}
                      className="mt-0.5 flex-none text-accent-text"
                    />
                    First saved version
                  </li>
                )}
                {starter === "blank" && !addReadme && !createInitialCommit && (
                  <li>No starter files</li>
                )}
              </ul>
              <div className="mt-4 border-t border-border pt-3">
                <div className="flex items-center gap-2 text-2xs text-sub">
                  <FolderGit2 size={12} />
                  Starts on{" "}
                  <span className="font-mono text-foreground">main</span>
                </div>
                <div className="mt-2 flex items-center gap-2 text-2xs text-sub">
                  <ShieldCheck size={12} />
                  Sent online{" "}
                  <span className="font-semibold text-foreground">No</span>
                </div>
              </div>
            </div>
          </section>
        </div>
      </div>
    </>
  );

  return (
    <div
      className={cn(
        "relative flex min-h-0 flex-1 overflow-hidden bg-background",
        wiggling && "gw-repo-picker-wiggle",
      )}
      onAnimationEnd={() => setWiggling(false)}
    >
      <div className="grid min-h-0 flex-1 grid-cols-1 grid-rows-[auto_minmax(0,1fr)] lg:grid-cols-[224px_minmax(0,1fr)] lg:grid-rows-1 xl:grid-cols-[244px_minmax(0,1fr)_320px]">
        <aside className="flex min-w-0 flex-col border-b border-border bg-panel px-3 py-3 lg:border-b-0 lg:border-r lg:px-3.5 lg:py-6">
          <div className="hidden px-2.5 lg:block">
            <h1 className="text-base font-semibold tracking-tight text-foreground">
              Add a repository
            </h1>
            <p className="mt-1.5 max-w-[190px] text-2xs leading-4 text-muted-foreground">
              Open work on this computer or bring in something new.
            </p>
          </div>
          <nav
            aria-label="Ways to add a repository"
            className="grid grid-cols-3 gap-1 lg:mt-6 lg:grid-cols-1 lg:gap-2"
          >
            <RouteButton
              active={route === "open"}
              icon={<FolderGit2 size={15} />}
              title="On this computer"
              detail="Browse or search your folders"
              shortcut="1"
              onClick={() => setRoute("open")}
            />
            <RouteButton
              active={route === "clone"}
              icon={<Download size={15} />}
              title="From a web address"
              detail="Copy a project to this PC"
              shortcut="2"
              onClick={() => setRoute("clone")}
            />
            <RouteButton
              active={route === "new"}
              icon={<Plus size={15} />}
              title="Start something new"
              detail="Create a ready-to-use folder"
              shortcut="3"
              onClick={() => setRoute("new")}
            />
          </nav>
          <span className="hidden flex-1 lg:block" />
          <section className="mt-5 hidden rounded-lg border border-border bg-background p-3.5 shadow-[0_8px_24px_rgba(0,0,0,.08)] lg:block">
            <div className="flex items-center gap-1.5 text-[9px] font-bold uppercase tracking-[.09em] text-muted-foreground">
              <Eye size={11} />
              Watched folder
            </div>
            {codeFolder ? (
              <>
                <div className="mt-2 truncate font-mono text-xs font-semibold text-foreground">
                  {codeFolder}
                </div>
                <p className="mt-1 text-2xs text-muted-foreground">
                  {scanned.isLoading
                    ? "Finding repositories…"
                    : `${scanned.data?.length ?? 0} repositories found automatically`}
                </p>
              </>
            ) : (
              <p className="mt-2 text-2xs leading-4 text-muted-foreground">
                Choose a folder to find repositories automatically.
              </p>
            )}
            <div className="mt-3 flex gap-1.5">
              <Button
                variant="secondary"
                size="sm"
                className="h-7 px-2 text-2xs"
                onClick={pickCodeFolder}
              >
                {codeFolder ? "Change" : "Choose"}
              </Button>
              {codeFolder && (
                <Button
                  variant="ghost"
                  size="sm"
                  className="h-7 gap-1 px-2 text-2xs"
                  onClick={() => {
                    scanned.refetch();
                    toast("Rescanning watched folder…");
                  }}
                >
                  <RefreshCw
                    size={11}
                    className={cn(scanned.isFetching && "animate-spin")}
                  />
                  Rescan
                </Button>
              )}
            </div>
          </section>
        </aside>

        <main
          className="relative flex min-h-0 min-w-0 flex-col"
          aria-live="polite"
        >
          {route === "open"
            ? openScreen
            : route === "clone"
              ? cloneScreen
              : newScreen}
        </main>

        <aside className="hidden min-h-0 flex-col border-l border-border bg-panel xl:flex">
          {route === "open" ? (
            <RepoDetails
              selected={selectedItem}
              repositories={libraryRepos}
              groups={savedTabGroups}
              openByPath={openByPath}
              pinnedRepoPaths={pinnedRepoPaths}
              openingGroupId={openingGroupId}
              busy={busy}
              onToggleRepoPin={(path) => {
                const repo = repoByPath.get(pathKey(path));
                if (repo) toggleRepoPin(repo);
              }}
              onOpenRepo={openOne}
              onJumpToRepo={jumpToRepo}
              onOpenGroup={(id) => void openSavedGroup(id)}
            />
          ) : route === "clone" ? (
            <>
              <div className="border-b border-border px-5 py-4 text-2xs font-bold uppercase tracking-[.09em] text-muted-foreground">
                What happens next
              </div>
              <div className="p-5">
                <span className="grid size-10 place-items-center rounded-lg border border-accent/25 bg-accent/10 text-accent-text">
                  <Download size={19} />
                </span>
                <h2 className="mt-4 text-base font-semibold text-foreground">
                  Ready when the copy finishes
                </h2>
                <p className="mt-2 text-xs leading-5 text-muted-foreground">
                  GitWyrm copies the files, remembers the parent folder, and
                  opens the repository in a new tab.
                </p>
                <div className="mt-5 border-t border-border pt-4">
                  <div className="flex items-center gap-2 text-xs text-sub">
                    <ShieldCheck size={13} className="text-accent-text" />
                    Uses your normal Git sign-in
                  </div>
                </div>
              </div>
            </>
          ) : (
            <>
              <div className="border-b border-border px-5 py-4 text-2xs font-bold uppercase tracking-[.09em] text-muted-foreground">
                A clean starting point
              </div>
              <div className="p-5">
                <span className="grid size-10 place-items-center rounded-lg border border-accent/25 bg-accent/10 text-accent-text">
                  <FolderPlus size={19} />
                </span>
                <h2 className="mt-4 text-base font-semibold text-foreground">
                  Ready for your first change
                </h2>
                <p className="mt-2 text-xs leading-5 text-muted-foreground">
                  GitWyrm creates the folder on main and opens it in a new tab.
                  Nothing is sent online.
                </p>
              </div>
            </>
          )}
        </aside>
      </div>

      <GroupLibrary
        open={groupLibraryOpen}
        onOpenChange={setGroupLibraryOpen}
        groups={savedTabGroups}
        pinnedIds={pinnedSavedGroupIds}
        openingGroupId={openingGroupId}
        onTogglePin={toggleGroupPin}
        onOpenGroup={(id) => void openSavedGroup(id)}
        onDelete={(id) => {
          const group = savedTabGroups.find((candidate) => candidate.id === id);
          deleteSavedTabGroup(id);
          if (selectedItem?.type === "group" && selectedItem.id === id) {
            setSelectedItem(null);
          }
          if (group) toast.success(`${group.name} removed from saved groups`);
        }}
      />
    </div>
  );
}

export function RepoPickerView() {
  // Finishing the job retires the tab; clicking away to another tab does not,
  // so a half-typed clone URL survives the trip.
  const closeRepoPicker = useUiStore((state) => state.closeRepoPicker);
  const wiggleNonce = useUiStore((state) => state.repoPickerWiggleNonce);
  return <RepoPickerPanel onDone={closeRepoPicker} wiggleNonce={wiggleNonce} />;
}
