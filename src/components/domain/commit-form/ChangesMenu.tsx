import { type ReactNode, useState } from 'react'
import { Archive, MinusCircle, PlusCircle, Sparkles, Trash2 } from 'lucide-react'
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuLabel,
  ContextMenuSeparator,
  ContextMenuTrigger,
} from '@/components/ui/context-menu'
import { PendingMenuItem } from '@/components/ui/pending-menu-item'
import { useStatus } from '@/hooks/useGitQueries'
import { useGitMutations } from '@/hooks/useGitMutations'
import { useActiveRepo } from '@/stores/workspaceStore'
import { plural } from '@/lib/gitDisplay'
import { workingChangesSourceInput } from '@/lib/agentDeskSources'
import { useStartAgentSession } from '@/hooks/useStartAgentSession'
import { DiscardAllDialog } from './DiscardAllDialog'

interface ChangesMenuProps {
  children: ReactNode
  /** Radix trigger mode: right-click a row, or as a button dropdown target. */
  asChild?: boolean
}

/** Right-click menu for the whole set of uncommitted changes. */
export function ChangesMenu({ children, asChild = true }: ChangesMenuProps) {
  const repo = useActiveRepo()
  const status = useStatus(repo?.id ?? null)
  const m = useGitMutations(repo?.id ?? null)
  const [confirmDiscard, setConfirmDiscard] = useState(false)
  const { startSession } = useStartAgentSession()

  const staged = status.data?.staged.length ?? 0
  const unstaged = status.data?.unstaged.length ?? 0
  const total = staged + unstaged
  const hasChanges = total > 0
  const operationPending =
    m.stageAll.isPending || m.unstageAll.isPending || m.stashSave.isPending

  // One entry per file. A file edited after being staged appears in both
  // lists, and asking an agent about the same path twice reads as two
  // different files.
  const changedPaths = [
    ...new Set([
      ...(status.data?.staged ?? []).map((f) => f.path),
      ...(status.data?.unstaged ?? []).map((f) => f.path),
    ]),
  ]

  return (
    <>
      <ContextMenu>
        <ContextMenuTrigger asChild={asChild}>{children}</ContextMenuTrigger>
        <ContextMenuContent className="w-52">
          <ContextMenuLabel className="text-2xs text-sub">
            {hasChanges ? `${plural(total, "changed file")}` : "No changes"}
          </ContextMenuLabel>
          <ContextMenuSeparator />
          <PendingMenuItem
            icon={<PlusCircle />}
            label="Stage all changes"
            pendingLabel="Staging all changes…"
            pending={m.stageAll.isPending}
            disabled={unstaged === 0 || operationPending}
            onRun={() => m.stageAll.mutate()}
          />
          <PendingMenuItem
            icon={<MinusCircle />}
            label="Unstage all"
            pendingLabel="Unstaging all…"
            pending={m.unstageAll.isPending}
            disabled={staged === 0 || operationPending}
            onRun={() => m.unstageAll.mutate()}
          />
          <PendingMenuItem
            icon={<Archive />}
            label="Stash all changes"
            pendingLabel="Stashing changes…"
            pending={m.stashSave.isPending}
            disabled={!hasChanges || operationPending}
            onRun={() => m.stashSave.mutate(undefined)}
          />
          {/* The uncommitted changes as a starting point. The source kind has
              been stored end to end since sources shipped, and this is the
              gesture that was missing -- the same shape the commit menu uses
              to explain a commit.

              `explain` rather than a writing intent: this asks about work the
              person has in progress, and an agent that started editing their
              uncommitted files unasked would be the worst possible surprise. */}
          {repo && hasChanges && (
            <>
              <ContextMenuSeparator />
              <ContextMenuItem
                onSelect={() =>
                  void startSession({
                    key: `workingChanges:${repo.id}`,
                    repoId: repo.id,
                    repoPath: repo.path,
                    repoName: repo.name,
                    intent: 'explain',
                    source: workingChangesSourceInput(changedPaths),
                  })
                }
              >
                <Sparkles />
                Explain with AI
              </ContextMenuItem>
            </>
          )}
          <ContextMenuSeparator />
          <ContextMenuItem
            variant="destructive"
            disabled={!hasChanges}
            onSelect={() => setConfirmDiscard(true)}
          >
            <Trash2 />
            Discard all changes
          </ContextMenuItem>
        </ContextMenuContent>
      </ContextMenu>

      <DiscardAllDialog open={confirmDiscard} onOpenChange={setConfirmDiscard} />
    </>
  )
}
