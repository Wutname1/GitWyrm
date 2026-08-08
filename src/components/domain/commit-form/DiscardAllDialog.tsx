import { ConfirmDialog } from "@/components/modals/ConfirmDialog";
import { useStatus } from "@/hooks/useGitQueries";
import { useGitMutations } from "@/hooks/useGitMutations";
import { useActiveRepo, useWorkspaceStore } from "@/stores/workspaceStore";
import { plural } from "@/lib/gitDisplay";

interface DiscardAllDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

/**
 * The confirmation behind "Discard all changes", shared by the trash button and
 * the Changes right-click menu so both offer the same choices.
 *
 * Checking out the parent's tree cannot move a nested checkout, so a submodule
 * the user moved survives a discard unless it is reset on purpose. The box that
 * does that only appears when a submodule has actually moved, and its state is
 * remembered app-wide -- someone who deliberately parks a submodule elsewhere
 * unticks it once.
 */
export function DiscardAllDialog({ open, onOpenChange }: DiscardAllDialogProps) {
  const repo = useActiveRepo();
  const status = useStatus(repo?.id ?? null);
  const m = useGitMutations(repo?.id ?? null);
  const resetSubmodules = useWorkspaceStore((s) => s.discardResetsSubmodules);
  const setResetSubmodules = useWorkspaceStore(
    (s) => s.setDiscardResetsSubmodules,
  );

  const files = [
    ...(status.data?.staged ?? []),
    ...(status.data?.unstaged ?? []),
  ];
  const changedFiles = new Set(files.map((file) => file.path)).size;
  const movedSubmodules = new Set(
    files.filter((file) => file.submodule).map((file) => file.path),
  ).size;
  const willReset = movedSubmodules > 0 && resetSubmodules;

  return (
    <ConfirmDialog
      open={open}
      onOpenChange={onOpenChange}
      destructive
      title="Discard all changes?"
      description={
        <>
          This throws away every uncommitted change across{" "}
          <span className="text-foreground">{changedFiles}</span> file
          {changedFiles === 1 ? "" : "s"} and puts your project back to the last
          commit. This can't be undone. Consider stashing instead.
        </>
      }
      extra={
        movedSubmodules > 0 ? (
          <label className="flex cursor-pointer items-center gap-2 text-xs text-sub">
            <input
              type="checkbox"
              checked={resetSubmodules}
              onChange={(e) => setResetSubmodules(e.target.checked)}
              className="size-3.5 accent-[var(--gw-accent)]"
            />
            Also send {plural(movedSubmodules, "submodule")} back to the
            recorded commit
          </label>
        ) : undefined
      }
      confirmLabel="Discard everything"
      pending={m.discardAll.isPending}
      pendingLabel="Discarding changes…"
      keepOpenOnConfirm
      onConfirm={() =>
        m.discardAll.mutate(willReset, {
          onSuccess: () => onOpenChange(false),
        })
      }
    />
  );
}
