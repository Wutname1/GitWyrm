import { useState, type ReactNode } from "react";
import {
  FolderTree,
  GitCommitHorizontal,
  List,
  Pencil,
  Trash2,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import { TooltipButton } from "@/components/ui/tooltip";
import { PendingIndicator } from "@/components/ui/pending-indicator";
import { ConfirmDialog } from "@/components/modals/ConfirmDialog";
import { useStatus } from "@/hooks/useGitQueries";
import { useGitMutations } from "@/hooks/useGitMutations";
import { useUiStore } from "@/stores/uiStore";
import {
  useActiveRepo,
  useWorkspaceStore,
  type ChangesViewMode,
} from "@/stores/workspaceStore";
import { plural } from "@/lib/gitDisplay";
import { cn } from "@/lib/utils";
import { FileChangeRow, StageToggle } from "../FileChangeRow";
import { GenerateCommitsDialog } from "./GenerateCommitsDialog";
import { FileChangeTree } from "./FileChangeTree";
import { ChangesMenu } from "./ChangesMenu";

function GroupHeader({
  label,
  count,
  tone,
  children,
}: {
  label: string;
  count: number;
  tone: "staged" | "unstaged";
  children: ReactNode;
}) {
  return (
    <ChangesMenu>
      {/* Sits below the sticky changes header, which is ~34px tall. */}
      <div className="sticky top-[34px] z-[2] flex items-center gap-2 bg-panel2/40 px-3.5 py-[7px]">
        {tone === "staged" ? (
          <GitCommitHorizontal
            size={16}
            strokeWidth={2}
            className="flex-none text-modified"
            aria-hidden
          />
        ) : (
          <Pencil
            size={15}
            strokeWidth={2}
            className="flex-none text-modified"
            aria-hidden
          />
        )}
        <span className="text-2xs font-bold tracking-[.05em] text-sub">
          {label}
        </span>
        <span className="font-mono text-2xs text-muted-foreground">
          {count}
        </span>
        <div className="ml-auto flex items-center gap-1">{children}</div>
      </div>
    </ChangesMenu>
  );
}

/** Two-way switch between the folder tree and the flat file list. */
function ViewModeToggle({
  mode,
  onChange,
}: {
  mode: ChangesViewMode;
  onChange: (mode: ChangesViewMode) => void;
}) {
  const options: Array<{
    value: ChangesViewMode;
    label: string;
    tooltip: string;
    Icon: typeof FolderTree;
  }> = [
    {
      value: "tree",
      label: "Tree",
      tooltip: "Group changed files by folder",
      Icon: FolderTree,
    },
    {
      value: "list",
      label: "List",
      tooltip: "Show every changed file in one flat list",
      Icon: List,
    },
  ];

  return (
    <div
      role="radiogroup"
      aria-label="Changed file layout"
      className="flex items-center gap-0.5 rounded border border-border bg-panel2 p-0.5"
    >
      {options.map(({ value, label, tooltip, Icon }) => (
        <TooltipButton
          key={value}
          type="button"
          role="radio"
          aria-checked={mode === value}
          aria-label={label}
          tooltip={tooltip}
          tooltipSide="top"
          onClick={() => onChange(value)}
          className={cn(
            "flex size-[18px] flex-none items-center justify-center rounded p-0 text-sub hover:bg-panel3 hover:text-foreground",
            mode === value && "bg-soft text-accent-text hover:bg-soft",
          )}
        >
          <Icon className="size-3" />
        </TooltipButton>
      ))}
    </div>
  );
}

export function ChangesList() {
  const [confirmDiscard, setConfirmDiscard] = useState(false);
  const repo = useActiveRepo();
  const status = useStatus(repo?.id ?? null);
  const openDiff = useUiStore((s) => s.openDiff);
  const openConflict = useUiStore((s) => s.openConflict);
  const changesViewMode = useWorkspaceStore((s) => s.changesViewMode);
  const setChangesViewMode = useWorkspaceStore((s) => s.setChangesViewMode);
  const m = useGitMutations(repo?.id ?? null);

  const staged = status.data?.staged ?? [];
  const unstaged = status.data?.unstaged ?? [];
  const allFiles = [...staged, ...unstaged];
  const hasChanges = staged.length > 0 || unstaged.length > 0;
  const changedFiles = new Set(
    [...staged, ...unstaged].map((file) => file.path),
  ).size;
  const hasConflicts = unstaged.some((file) => file.conflicted);
  const stagingPending =
    m.stageFile.isPending ||
    m.unstageFile.isPending ||
    m.stageFiles.isPending ||
    m.unstageFiles.isPending ||
    m.stageAll.isPending ||
    m.unstageAll.isPending ||
    m.discardFiles.isPending ||
    m.discardAll.isPending;

  return (
    <>
      <div className="min-h-0 flex-1 overflow-y-auto">
        <ChangesMenu>
          <div className="sticky top-0 z-[3] flex items-center gap-2 border-b border-border/70 bg-panel px-3.5 py-2">
            <span className="text-xs font-semibold text-foreground">
              {hasChanges ? plural(changedFiles, "change") : "No changes"}
            </span>
            <div className="ml-auto flex items-center gap-1">
              <ViewModeToggle
                mode={changesViewMode}
                onChange={setChangesViewMode}
              />
              {hasChanges && (
                <Button
                  variant="ghost"
                  size="icon-xs"
                  tooltip="Discard all changes"
                  tooltipSide="top"
                  onClick={() => setConfirmDiscard(true)}
                  disabled={stagingPending}
                  className="h-5 w-5 rounded text-removed hover:bg-removed/10 hover:text-removed"
                >
                  <Trash2 className="size-3" />
                </Button>
              )}
            </div>
          </div>
        </ChangesMenu>
        <GroupHeader label="UNSTAGED" count={unstaged.length} tone="unstaged">
          {unstaged.length > 0 && (
            <Button
              size="sm"
              onClick={() => m.stageAll.mutate()}
              disabled={stagingPending}
              className="h-auto rounded border border-primary/50 bg-soft px-2 py-0.5 text-2xs font-semibold text-accent-text hover:border-primary hover:bg-primary hover:text-primary-foreground"
            >
              {m.stageAll.isPending && <PendingIndicator className="size-3" />}
              {m.stageAll.isPending ? "Staging…" : "Stage all"}
            </Button>
          )}
        </GroupHeader>
        {unstaged.length > 0 && (
          <FileChangeTree
            files={unstaged}
            allFiles={allFiles}
            treeId="unstaged"
            staged={false}
            viewMode={changesViewMode}
            operationsDisabled={stagingPending}
            mutations={m}
            renderFile={(f, name, depth) =>
              f.conflicted ? (
                <FileChangeRow
                  file={f}
                  displayPath={name}
                  treeDepth={depth}
                  menuStaged={false}
                  nameClassName="text-removed"
                  onOpen={() => openConflict(f.path)}
                  action={
                    <button
                      onClick={(e) => {
                        e.stopPropagation();
                        openConflict(f.path);
                      }}
                      className="flex-none rounded border border-removed/50 bg-removed/10 px-1.5 py-0.5 text-2xs font-semibold text-removed hover:bg-removed/20"
                    >
                      Resolve
                    </button>
                  }
                />
              ) : (
                <FileChangeRow
                  file={f}
                  displayPath={name}
                  treeDepth={depth}
                  menuStaged={false}
                  onOpen={() =>
                    openDiff({ path: f.path, source: { kind: "unstaged" } })
                  }
                  action={
                    <StageToggle
                      direction="stage"
                      disabled={stagingPending}
                      pending={
                        m.stageFile.isPending &&
                        m.stageFile.variables === f.path
                      }
                      onToggle={(e) => {
                        e.stopPropagation();
                        m.stageFile.mutate(f.path);
                      }}
                    />
                  }
                />
              )
            }
          />
        )}
        {status.data && !hasChanges && (
          <div className="p-4 text-center text-2xs text-muted-foreground">
            Working tree clean
          </div>
        )}
        <div className="my-1 border-t-2 border-border/70" />
        <GroupHeader label="STAGED" count={staged.length} tone="staged">
          {staged.length > 0 && (
            <Button
              variant="ghost"
              size="sm"
              onClick={() => m.unstageAll.mutate()}
              disabled={stagingPending}
              className="h-auto rounded px-[7px] py-0.5 text-2xs text-sub hover:bg-panel3 hover:text-foreground border-primary/40 border"
            >
              {m.unstageAll.isPending && (
                <PendingIndicator className="size-3" />
              )}
              {m.unstageAll.isPending ? "Unstaging…" : "Unstage all"}
            </Button>
          )}
        </GroupHeader>
        {staged.length > 0 && (
          <FileChangeTree
            files={staged}
            allFiles={allFiles}
            treeId="staged"
            staged
            viewMode={changesViewMode}
            operationsDisabled={stagingPending}
            mutations={m}
            renderFile={(f, name, depth) => (
              <FileChangeRow
                file={f}
                displayPath={name}
                treeDepth={depth}
                menuStaged
                onOpen={() =>
                  openDiff({ path: f.path, source: { kind: "staged" } })
                }
                action={
                  <StageToggle
                    direction="unstage"
                    disabled={stagingPending}
                    pending={
                      m.unstageFile.isPending &&
                      m.unstageFile.variables === f.path
                    }
                    onToggle={(e) => {
                      e.stopPropagation();
                      m.unstageFile.mutate(f.path);
                    }}
                  />
                }
              />
            )}
          />
        )}
        {staged.length === 0 && hasChanges && (
          <div className="px-3.5 py-1.5 text-2xs italic text-muted-foreground">
            Nothing staged yet
          </div>
        )}
      </div>

      <GenerateCommitsDialog
        changedFiles={changedFiles}
        hasConflicts={hasConflicts}
        canOffer={hasChanges && staged.length === 0}
      />

      <ConfirmDialog
        open={confirmDiscard}
        onOpenChange={setConfirmDiscard}
        destructive
        title="Discard all changes?"
        description={
          <>
            This throws away every uncommitted change across{" "}
            <span className="text-foreground">{changedFiles}</span> file
            {changedFiles === 1 ? "" : "s"} and puts your project back to the
            last commit. This can't be undone. Consider stashing instead.
          </>
        }
        confirmLabel="Discard everything"
        pending={m.discardAll.isPending}
        pendingLabel="Discarding changes…"
        keepOpenOnConfirm
        onConfirm={() =>
          m.discardAll.mutate(undefined, {
            onSuccess: () => setConfirmDiscard(false),
          })
        }
      />
    </>
  );
}
