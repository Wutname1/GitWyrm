import { type ReactNode, useEffect, useRef, useState } from "react";
import {
  ArchiveRestore,
  Archive,
  ArrowDown,
  ArrowDownToLine,
  ArrowUp,
  ChevronDown,
  ChevronUp,
  Folder,
  FolderOpen,
  GitBranch,
  GitMerge,
  Search,
  SquareTerminal,
  X,
} from "lucide-react";
import { Separator } from "@/components/ui/separator";
import { PendingIndicator } from "@/components/ui/pending-indicator";
import {
  DisabledHint,
  TooltipButton,
  TooltipHint,
} from "@/components/ui/tooltip";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { detectProvider, RemoteIcon } from "@/lib/remoteProvider";
import { cn } from "@/lib/utils";
import { branchSync } from "@/lib/branchActions";
import { pullNeedsChoice } from "@/lib/syncPreview";
import { SyncBadge } from "@/components/domain/branch/SyncBadge";
import { OpenInEditorButton } from "@/components/domain/OpenInEditorButton";
import { OpenSpecDeskButton } from "@/components/domain/OpenSpecDeskButton";
import { useBranches, useRemotes, useStashes } from "@/hooks/useGitQueries";
import { useGitMutations } from "@/hooks/useGitMutations";
import { useUiStore } from "@/stores/uiStore";
import { useActiveRepo } from "@/stores/workspaceStore";

interface ToolbarButtonProps {
  icon: ReactNode;
  label: string;
  badge?: string;
  onClick: () => void;
  disabled?: boolean;
  pending?: boolean;
  /** Why the button is off, shown on hover in place of the usual tooltip. */
  reason?: string;
}

function ToolbarButton({
  icon,
  label,
  badge,
  onClick,
  disabled,
  pending,
  reason,
}: ToolbarButtonProps) {
  return (
    <DisabledHint disabled={disabled} reason={reason}>
      <TooltipButton
        onClick={onClick}
        tooltip={disabled && reason ? reason : label}
        // Keep the name describing what the button does. Without this the
        // name would fall back to the reason, leaving every disabled button
        // in the toolbar announced identically.
        aria-label={label}
        disabled={disabled}
        aria-busy={pending || undefined}
        className={cn(
          "relative flex h-8 items-center gap-[7px] overflow-hidden rounded-md border border-transparent px-[11px] text-foreground transition-[border-color,background-color,color,opacity] hover:border-muted-foreground hover:bg-panel3 disabled:pointer-events-none",
          disabled && !pending && "opacity-35",
          pending &&
            "wyrm-operation-active border-primary/40 bg-soft text-accent-text",
        )}
      >
        <span className="relative flex flex-none">
          {pending ? <PendingIndicator /> : icon}
          {badge && !pending && (
            <span className="wyrm-sync-pulse absolute -right-[9px] -top-[7px] rounded-full bg-primary px-1 font-mono text-2xs font-bold leading-[1.3] text-primary-foreground">
              {badge}
            </span>
          )}
        </span>
        <span className="text-xs font-medium">{label}</span>
      </TooltipButton>
    </DisabledHint>
  );
}

/**
 * The current-branch pill by the search box, doubling as a dropdown of every
 * branch. Clicking any branch switches to it; picking a remote one lands on a
 * local branch tracking it.
 */
// Explicit "switch to this branch" affordance on a dropdown row. The whole row
// already switches on click; this button makes the action discoverable. It sits
// hidden until the row is hovered so rows stay clean.
function SwitchRowButton({ onClick }: { onClick: () => void }) {
  return (
    <TooltipHint label="Switch to this branch">
      <span
        role="button"
        tabIndex={-1}
        onClick={(e) => {
          e.stopPropagation();
          onClick();
        }}
        className="flex-none rounded p-0.5 text-muted-foreground opacity-0 hover:bg-panel3 hover:text-foreground focus:opacity-100 group-hover:opacity-100"
      >
        <GitBranch size={12} strokeWidth={2} />
      </span>
    </TooltipHint>
  );
}

function GhostButton({
  icon,
  tooltip,
  onClick,
  disabled,
  reason,
}: {
  icon: ReactNode;
  tooltip: string;
  onClick: () => void;
  disabled?: boolean;
  reason?: string;
}) {
  return (
    <DisabledHint disabled={disabled} reason={reason}>
      <TooltipButton
        onClick={onClick}
        tooltip={disabled && reason ? reason : tooltip}
        aria-label={tooltip}
        disabled={disabled}
        className={cn(
          "group flex h-[30px] w-8 items-center justify-center rounded-md border border-border bg-panel2 text-sub hover:border-muted-foreground hover:bg-panel3 disabled:pointer-events-none",
          disabled && "opacity-35",
        )}
      >
        {icon}
      </TooltipButton>
    </DisabledHint>
  );
}

// A prev/next stepper button in the search box. Disabled when there's nothing
// to step through so it reads as inert rather than broken.
function StepButton({
  icon,
  title,
  onClick,
  disabled,
}: {
  icon: ReactNode;
  title: string;
  onClick: () => void;
  disabled: boolean;
}) {
  // DisabledHint rather than a plain tooltip: this button is disabled whenever
  // there is nothing to step to, and a disabled button gets no pointer events,
  // so its own tooltip could never open.
  return (
    <DisabledHint disabled={disabled} reason={title}>
      <TooltipButton
        onClick={onClick}
        disabled={disabled}
        tooltip={title}
        className="flex-none rounded p-0.5 hover:bg-panel3 hover:text-foreground disabled:pointer-events-none disabled:opacity-30"
      >
        {icon}
      </TooltipButton>
    </DisabledHint>
  );
}

/**
 * The commit search box. Typing dims commits in the graph that don't match and
 * scrolls to the first that does. A "current/total" readout and up/down steppers
 * move between matches (Enter / Shift+Enter do the same). Ctrl+F focuses it (via
 * a nonce the app root bumps).
 */
function CommitSearchBox() {
  const query = useUiStore((s) => s.commitSearch);
  const setCommitSearch = useUiStore((s) => s.setCommitSearch);
  const jumpMatch = useUiStore((s) => s.jumpMatch);
  const focusNonce = useUiStore((s) => s.searchFocusNonce);
  const matchCount = useUiStore((s) => s.searchMatchCount);
  const matchIndex = useUiStore((s) => s.searchMatchIndex);
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (focusNonce === 0) return;
    inputRef.current?.focus();
    inputRef.current?.select();
  }, [focusNonce]);

  const hasMatches = (matchCount ?? 0) > 0;
  const showStatus = query.trim() !== "" && matchCount !== null;

  return (
    <div className="flex h-[30px] min-w-[210px] items-center gap-[7px] rounded-md border border-border bg-panel2 px-2.5 text-muted-foreground focus-within:border-muted-foreground">
      <Search size={14} strokeWidth={1.9} className="flex-none" />
      <input
        ref={inputRef}
        value={query}
        onChange={(e) => setCommitSearch(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Enter") {
            e.preventDefault();
            if (hasMatches) jumpMatch(e.shiftKey ? -1 : 1);
          } else if (e.key === "Escape") {
            setCommitSearch("");
            inputRef.current?.blur();
          }
        }}
        placeholder="Search commits"
        className="min-w-0 flex-1 bg-transparent text-xs text-foreground outline-none placeholder:text-muted-foreground"
      />
      {showStatus && (
        <span
          className={cn(
            "flex-none whitespace-nowrap font-mono text-2xs tabular-nums",
            hasMatches ? "text-muted-foreground" : "text-removed",
          )}
        >
          {hasMatches ? `${matchIndex || 1}/${matchCount}` : "no results"}
        </span>
      )}
      {showStatus && hasMatches && (
        <>
          <StepButton
            icon={<ChevronUp size={13} strokeWidth={2} />}
            title="Previous match (Shift+Enter)"
            onClick={() => jumpMatch(-1)}
            disabled={!hasMatches}
          />
          <StepButton
            icon={<ChevronDown size={13} strokeWidth={2} />}
            title="Next match (Enter)"
            onClick={() => jumpMatch(1)}
            disabled={!hasMatches}
          />
        </>
      )}
      {query && (
        <TooltipButton
          onClick={() => {
            setCommitSearch("");
            inputRef.current?.focus();
          }}
          tooltip="Clear search"
          className="flex-none rounded p-0.5 hover:bg-panel3 hover:text-foreground"
        >
          <X size={13} strokeWidth={2} />
        </TooltipButton>
      )}
    </div>
  );
}

export function Toolbar() {
  const repo = useActiveRepo();
  const branches = useBranches(repo?.id ?? null);
  const stashes = useStashes(repo?.id ?? null);
  const m = useGitMutations(repo?.id ?? null);
  const openMerge = useUiStore((s) => s.openMerge);
  const openModal = useUiStore((s) => s.openModal);
  const openRemoteSync = useUiStore((s) => s.openRemoteSync);
  const head = branches.data?.local.find((b) => b.is_head);
  const syncAction = m.fetch.isPending
    ? "fetch"
    : m.pull.isPending
      ? "pull"
      : m.push.isPending
        ? "push"
        : null;
  const syncPending = syncAction !== null;
  // A branch that has never been sent has no count to show -- there is nothing
  // to be ahead of -- but it still has work to push, so mark the button rather
  // than leaving it looking as settled as a branch that matches its remote.
  const headSync = head ? branchSync(head) : null;
  const pushBadge =
    headSync?.marker === "new"
      ? "•"
      : headSync?.ahead
        ? String(headSync.ahead)
        : undefined;
  const canChooseSync = pullNeedsChoice({
    upstream: head?.upstream,
    ahead: headSync?.ahead ?? 0,
    behind: headSync?.behind ?? 0,
  });
  const stashAction = m.stashSave.isPending
    ? "stash"
    : m.stashPop.isPending
      ? "pop"
      : null;
  const stashPending = stashAction !== null;

  // With no repository open there is nothing for any of these to act on, so
  // they are switched off outright and say why on hover, rather than looking
  // live and answering a click with a toast.
  const noRepo = !repo;
  const noRepoReason = "Open a repository first";
  const noStashes = (stashes.data?.length ?? 0) === 0;

  return (
    <div
      data-dim-on-drag
      className="relative flex h-12 flex-none items-center gap-1 border-b border-border bg-panel px-2.5"
    >
      <ToolbarButton
        icon={<ArrowDownToLine size={16} strokeWidth={1.9} />}
        label={m.fetch.isPending ? "Fetching…" : "Fetch"}
        onClick={() => m.fetch.mutate(undefined)}
        disabled={noRepo || syncPending}
        reason={noRepoReason}
        pending={m.fetch.isPending}
      />
      <ToolbarButton
        icon={<ArrowDown size={16} strokeWidth={1.9} />}
        label={m.pull.isPending ? "Pulling…" : "Pull"}
        badge={headSync?.behind ? String(headSync.behind) : undefined}
        // With work on both sides, a plain pull silently writes a merge commit
        // -- a history decision made for the user. Hand that choice back: the
        // sync modal shows blend / stack / replace with the resulting shape.
        // A one-sided pull has no decision in it and still runs straight away.
        onClick={() =>
          canChooseSync
            ? openRemoteSync(head!.upstream!, head!.name)
            : m.pull.mutate()
        }
        disabled={noRepo || syncPending}
        reason={noRepoReason}
        pending={m.pull.isPending}
      />
      <ToolbarButton
        icon={<ArrowUp size={16} strokeWidth={1.9} />}
        label={m.push.isPending ? "Pushing…" : "Push"}
        badge={pushBadge}
        // A branch that is behind its upstream can't be sent with a plain push --
        // git refuses it as non-fast-forward. Catch that here and let the user
        // choose (get the cloud's changes first, or force past them) rather than
        // firing a push we know will be rejected.
        onClick={() =>
          headSync && headSync.behind > 0
            ? openModal("push-choice")
            : m.push.mutate()
        }
        disabled={noRepo || syncPending}
        reason={noRepoReason}
        pending={m.push.isPending}
      />

      {syncAction && (
        <div
          className={cn(
            "wyrm-sync-track pointer-events-none absolute bottom-0 left-2.5 w-[224px]",
            syncAction === "push"
              ? "wyrm-sync-track-out"
              : "wyrm-sync-track-in",
          )}
          aria-hidden="true"
        >
          <span />
        </div>
      )}
      <span className="sr-only" role="status" aria-live="polite">
        {syncAction === "fetch"
          ? "Fetching from the remote"
          : syncAction === "pull"
            ? "Pulling changes from the remote"
            : syncAction === "push"
              ? "Pushing changes to the remote"
              : ""}
      </span>

      <Separator orientation="vertical" className="mx-1.5 !h-[26px]" />

      <ToolbarButton
        icon={<GitBranch size={16} strokeWidth={1.9} />}
        label="Branch"
        onClick={() => openModal("newBranch")}
        disabled={noRepo}
        reason={noRepoReason}
      />
      <ToolbarButton
        icon={<GitMerge size={16} strokeWidth={1.9} />}
        label="Merge"
        onClick={() => openMerge()}
        disabled={noRepo}
        reason={noRepoReason}
      />
      <ToolbarButton
        icon={<Archive size={16} strokeWidth={1.9} />}
        label={m.stashSave.isPending ? "Stashing…" : "Stash"}
        onClick={() => m.stashSave.mutate(undefined)}
        disabled={noRepo || stashPending}
        reason={noRepoReason}
        pending={m.stashSave.isPending}
      />
      <ToolbarButton
        icon={<ArchiveRestore size={16} strokeWidth={1.9} />}
        label={m.stashPop.isPending ? "Restoring…" : "Pop"}
        onClick={() => m.stashPop.mutate(0)}
        disabled={noRepo || noStashes || stashPending}
        reason={noRepo ? noRepoReason : "Nothing stashed to restore"}
        pending={m.stashPop.isPending}
      />

      <div className="flex-1" />

      <CommitSearchBox />

      <GhostButton
        icon={
          <span className="relative flex size-4 text-orange-400">
            <Folder
              size={16}
              strokeWidth={1.9}
              className="group-hover:hidden"
            />
            <FolderOpen
              size={16}
              strokeWidth={1.9}
              className="hidden group-hover:block"
            />
          </span>
        }
        tooltip="Show in file explorer"
        onClick={() => m.revealInFileManager.mutate()}
        disabled={noRepo}
        reason={noRepoReason}
      />
      <OpenInEditorButton disabled={noRepo} disabledReason={noRepoReason} />
      <OpenSpecDeskButton disabled={noRepo} disabledReason={noRepoReason} />
      <GhostButton
        icon={<SquareTerminal size={16} strokeWidth={1.9} />}
        tooltip="Open in terminal"
        onClick={() => m.openInTerminal.mutate()}
        disabled={noRepo}
        reason={noRepoReason}
      />
    </div>
  );
}
