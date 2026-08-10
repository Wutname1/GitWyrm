import { memo } from "react";
import { cn } from "@/lib/utils";
import type { CommitEntry, CommitStats } from "@/lib/bindings";
import { authorColor, formatCommitTime } from "@/lib/gitDisplay";
import {
  effectiveHiddenColumns,
  gridTemplate,
  isAuthorIconOnly,
  visibleColumns,
  type ColumnId,
} from "@/lib/graphColumns";
import { useWorkspaceStore } from "@/stores/workspaceStore";
import { Avatar } from "./Avatar";
import { AuthorHoverCard } from "./AuthorHoverCard";
import { RefBadge } from "./RefBadge";
import { RefStack } from "./RefStack";
import { CommitContextMenu } from "./CommitContextMenu";
import { MultiCommitContextMenu } from "./MultiCommitContextMenu";
import { ChangeSizeIndicator } from "./ChangeSizeIndicator";
import { AiCommitMarker, SpecChip } from "./SpecChip";
import { useOpenspecChanges } from "@/hooks/useOpenspec";
import { useActiveRepo } from "@/stores/workspaceStore";

/** Modifier keys held on a row click, for multi-select. */
export interface SelectModifiers {
  shift: boolean;
  /** Ctrl on Windows/Linux, Cmd on macOS. */
  ctrl: boolean;
}

interface CommitRowProps {
  commit: CommitEntry;
  /**
   * Diff stats fetched for this row after it scrolled into view, when the log
   * page did not already carry them. Falls back to whatever the commit itself
   * has, which is set once the backend has the numbers cached.
   */
  stats?: CommitStats | null;
  selected: boolean;
  /**
   * Selection handler shared by every row, called with this row's own sha.
   * Taking a stable function plus the sha -- rather than a per-row closure --
   * is what lets `memo` hold while scrolling; a fresh closure per render would
   * re-render every visible row on every scroll tick.
   */
  onSelectCommit: (sha: string, mods: SelectModifiers) => void;
  /**
   * All selected commits (graph order, newest first) when this row is part of
   * a multi-selection of 2+ commits; null otherwise. Switches the right-click
   * menu to actions that apply to the whole selection.
   */
  multiSelection?: CommitEntry[] | null;
  rowHeight: number;
  /** Faded out because a commit search is active and this row doesn't match. */
  dimmed?: boolean;
  /**
   * Virtualizer offset. Passed as a number rather than a style object so the
   * prop compares by value; a new object each render would defeat `memo`.
   */
  offsetY: number;
  /** Marks this row as a tutorial spotlight target. */
  tutorialId?: string;
}

/** Shared by every row; only the translate differs, applied per row. */
const ROW_BASE_STYLE: React.CSSProperties = {
  position: "absolute",
  top: 0,
  left: 0,
  right: 0,
};

export const CommitRow = memo(function CommitRow({
  commit,
  stats,
  selected,
  onSelectCommit,
  multiSelection,
  rowHeight,
  dimmed,
  offsetY,
  tutorialId,
}: CommitRowProps) {
  const order = useWorkspaceStore((s) => s.columnOrder);
  const hidden = useWorkspaceStore((s) => s.hiddenColumns);
  const widths = useWorkspaceStore((s) => s.columnWidths);
  const changeSizeDisplay = useWorkspaceStore((s) => s.changeSizeDisplay);
  const showChangeIndicator = useWorkspaceStore((s) => s.showChangeIndicator);
  const showLineCounts = useWorkspaceStore((s) => s.showChangeLineCounts);
  const effectiveHidden = effectiveHiddenColumns(
    hidden,
    showChangeIndicator,
    changeSizeDisplay,
  );
  const color = authorColor(commit.author_email || commit.author_name);
  // The commit's own numbers win when the page already had them; otherwise use
  // whatever the on-demand fetch returned. Null in both means still loading.
  const filesChanged = commit.files_changed ?? stats?.files_changed ?? null;
  const additions = commit.additions ?? stats?.additions ?? null;
  const deletions = commit.deletions ?? stats?.deletions ?? null;
  const authorIconOnly = isAuthorIconOnly(widths);
  const repo = useActiveRepo();
  // Every row calls this, but react-query dedupes by key, so the whole
  // virtualized list shares one cache entry and one fetch.
  const changes = useOpenspecChanges(repo?.id ?? null);
  // Progress belongs on the branch tip only: it describes the change's state
  // now, which would be a lie on an older commit that merely mentions it.
  const isBranchTip = commit.refs.some(
    (r) => r.type === "branch" || r.type === "head",
  );
  const progress = isBranchTip
    ? (changes.data?.find((c) => c.id === commit.spec_id)?.progress ?? null)
    : null;

  const cell: Record<ColumnId, React.ReactNode> = {
    refs: (
      <div className="gw-refs-cell flex items-center gap-1 overflow-hidden pr-1.5">
        {commit.refs.length > 1 ? (
          <RefStack refs={commit.refs} />
        ) : (
          commit.refs.map((r) => (
            <RefBadge key={`${r.type}:${r.name}`} refTag={r} expandOnHover />
          ))
        )}
      </div>
    ),
    graph: <div />,
    message: (
      <div
        data-dim-on-drag
        className={cn(
          "min-w-0 overflow-hidden pr-2.5 text-foreground ",
          showChangeIndicator &&
            changeSizeDisplay === "row" &&
            "flex h-full flex-col justify-center",
        )}
      >
        <div className="flex min-w-0 items-center gap-1.5">
          <span className="overflow-hidden text-ellipsis whitespace-nowrap">
            {commit.summary}
          </span>
          {commit.assisted_by && (
            <AiCommitMarker provider={commit.assisted_by} />
          )}
          {commit.spec_id && repo && (
            <SpecChip
              repoId={repo.id}
              changeId={commit.spec_id}
              progress={progress}
            />
          )}
        </div>
        {showChangeIndicator && changeSizeDisplay === "row" && (
          <ChangeSizeIndicator
            filesChanged={filesChanged}
            additions={additions}
            deletions={deletions}
            showLineCounts={showLineCounts}
            mode="row"
          />
        )}
      </div>
    ),
    author: (
      <AuthorHoverCard
        name={commit.author_name}
        email={commit.author_email}
        initials={commit.author_initials}
        sha={commit.sha}
      >
        <div
          data-dim-on-drag
          className="flex items-center gap-[7px] overflow-hidden"
        >
          <Avatar
            initials={commit.author_initials}
            color={color}
            email={commit.author_email}
          />
          {!authorIconOnly && (
            <span className="overflow-hidden text-ellipsis whitespace-nowrap text-2xs text-sub">
              {commit.author_name}
            </span>
          )}
        </div>
      </AuthorHoverCard>
    ),
    changes: (
      <ChangeSizeIndicator
        filesChanged={filesChanged}
        additions={additions}
        deletions={deletions}
        showLineCounts={showLineCounts}
        mode="column"
      />
    ),
    date: (
      <div data-dim-on-drag className="font-mono text-2xs text-sub">
        {formatCommitTime(commit.time)}
      </div>
    ),
    sha: (
      <div
        data-dim-on-drag
        className="font-mono text-2xs text-muted-foreground"
      >
        {commit.short_sha}
      </div>
    ),
  };

  const row = (
      <div
        data-tutorial-id={tutorialId}
        onClick={(e) =>
          onSelectCommit(commit.sha, { shift: e.shiftKey, ctrl: e.ctrlKey || e.metaKey })
        }
        // Right-clicking acts on the row under the cursor, so focus has to move
        // there or the menu describes one commit while the drawer shows another.
        // Radix opens on pointerdown, so this runs first and the menu is built
        // from the new selection.
        //
        // Already-selected rows are skipped, which also protects a
        // multi-selection: every row in one is `selected`, so right-clicking a
        // group keeps it intact instead of collapsing to the clicked commit.
        onContextMenu={() => {
          if (!selected) onSelectCommit(commit.sha, { shift: false, ctrl: false });
        }}
        style={{
          ...ROW_BASE_STYLE,
          height: rowHeight,
          gridTemplateColumns: gridTemplate(order, effectiveHidden, widths),
          transform: `translateY(${offsetY}px)`,
        }}
        className={cn(
          "grid cursor-pointer items-center pr-1 transition-opacity",
          // Alternating tint. The graph column can sit far from the message, so
          // a banded background keeps the eye on one row across the gap. Listed
          // before the selected/hover rules so those still win.
          "odd:bg-[color-mix(in_srgb,var(--gw-text)_3%,transparent)]",
          "hover:bg-[color-mix(in_srgb,var(--gw-text)_6%,transparent)]",
          selected && "bg-soft shadow-[inset_2px_0_0_var(--gw-accent)]",
          dimmed && "opacity-30",
        )}
      >
        {visibleColumns(order, effectiveHidden).map((id) => (
          <div key={id} className="contents">
            {cell[id]}
          </div>
        ))}
      </div>
  );

  if (multiSelection && multiSelection.length > 1) {
    return <MultiCommitContextMenu commits={multiSelection}>{row}</MultiCommitContextMenu>;
  }
  return (
    <CommitContextMenu
      commit={commit}
      onViewDetails={() => onSelectCommit(commit.sha, { shift: false, ctrl: false })}
    >
      {row}
    </CommitContextMenu>
  );
});
