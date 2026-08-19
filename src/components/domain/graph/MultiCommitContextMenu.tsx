import { type ReactNode, useState } from 'react'
import { Copy, GitBranchPlus, Layers, Pencil, Trash2, Undo2 } from 'lucide-react'
import { copyToClipboard } from '@/lib/clipboard'
import type { CommitEntry } from '@/lib/bindings'
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuLabel,
  ContextMenuSeparator,
  ContextMenuTrigger,
} from '@/components/ui/context-menu'
import { ConfirmDialog } from '@/components/modals/ConfirmDialog'
import { RewordDialog } from '@/components/modals/RewordDialog'
import { FormDialog } from '@/components/ui/form-dialog'
import { Input } from '@/components/ui/input'
import { PendingMenuItem } from '@/components/ui/pending-menu-item'
import { useBranches, useMergeState } from '@/hooks/useGitQueries'
import { useGitMutations } from '@/hooks/useGitMutations'
import { useUiStore } from '@/stores/uiStore'
import { useActiveRepo } from '@/stores/workspaceStore'
import { plural } from '@/lib/gitDisplay'

interface MultiCommitContextMenuProps {
  /** The selected commits in graph order (newest first), always 2 or more. */
  commits: CommitEntry[]
  children: ReactNode
}

/**
 * Right-click menu shown when the clicked row is part of a multi-selection.
 * Every action applies to the whole selection: cherry-pick and revert loop the
 * single-commit commands in a safe order, squash and drop go through the
 * one-shot history rewrite commands.
 */
export function MultiCommitContextMenu({ commits, children }: MultiCommitContextMenuProps) {
  const repo = useActiveRepo()
  const branches = useBranches(repo?.id ?? null)
  const mergeState = useMergeState(repo?.id ?? null)
  const m = useGitMutations(repo?.id ?? null)
  const selectCommit = useUiStore((s) => s.selectCommit)
  const [pending, setPending] = useState<'squash' | 'drop' | 'prefix' | null>(null)
  const [prefix, setPrefix] = useState('')

  const shas = commits.map((c) => c.sha)
  const count = commits.length
  const current = branches.data?.local.find((b) => b.is_head)
  const branchName = current?.name ?? 'current'
  const opInProgress = mergeState.data?.merging ?? false
  const includesHead = commits.some((c) => c.refs.some((r) => r.type === 'head'))
  const busy =
    m.cherryPickMany.isPending ||
    m.revertMany.isPending ||
    m.squashCommits.isPending ||
    m.dropCommits.isPending ||
    m.prefixCommits.isPending
  const canCherryPick = !opInProgress && !includesHead && !busy
  const canRevert = !opInProgress && current != null && !busy
  // Squash/drop rewrite the branch below the selection; need a branch checked out.
  const canRewrite = !opInProgress && current != null && !busy

  // Prefill the combined message: the oldest commit's summary stays the
  // summary, the rest are listed in the body oldest-to-newest so nothing is
  // silently thrown away.
  const oldestFirst = [...commits].reverse()
  const squashSummary = oldestFirst[0]?.summary ?? ''
  const squashBody = oldestFirst
    .slice(1)
    .map((c) => c.summary)
    .join('\n')

  const copyShas = () =>
    void copyToClipboard(shas.join('\n'), `Copied ${plural(count, 'SHA')}`)

  // The rewritten history invalidates every selected sha, so a finished
  // squash/drop also clears the selection instead of pointing at ghosts.
  const clearAndClose = () => {
    setPending(null)
    selectCommit(null)
  }

  return (
    <>
      <ContextMenu>
        <ContextMenuTrigger asChild>{children}</ContextMenuTrigger>
        <ContextMenuContent className="w-72">
          <ContextMenuLabel className="text-2xs text-sub">
            {plural(count, 'commit')} selected
          </ContextMenuLabel>
          <ContextMenuSeparator />
          <ContextMenuItem
            disabled={!canRewrite}
            onSelect={(e) => {
              e.preventDefault()
              setPending('squash')
            }}
          >
            <Layers />
            Combine into one commit (squash)
          </ContextMenuItem>
          <ContextMenuItem
            disabled={!canRewrite}
            onSelect={(e) => {
              e.preventDefault()
              setPrefix('')
              setPending('prefix')
            }}
          >
            <Pencil />
            Add a prefix to {plural(count, 'message')}
          </ContextMenuItem>
          <PendingMenuItem
            icon={<GitBranchPlus />}
            label={`Cherry-pick ${plural(count, 'commit')} onto ${branchName}`}
            pendingLabel="Adding commits…"
            pending={m.cherryPickMany.isPending}
            disabled={!canCherryPick}
            onRun={() => m.cherryPickMany.mutate(shas)}
          />
          <PendingMenuItem
            icon={<Undo2 />}
            label={`Undo ${plural(count, 'commit')} (revert)`}
            pendingLabel="Undoing…"
            pending={m.revertMany.isPending}
            disabled={!canRevert}
            onRun={() => m.revertMany.mutate(shas)}
          />
          <ContextMenuItem
            variant="destructive"
            disabled={!canRewrite}
            onSelect={(e) => {
              e.preventDefault()
              setPending('drop')
            }}
          >
            <Trash2 />
            Drop {plural(count, 'commit')}
          </ContextMenuItem>
          <ContextMenuSeparator />
          <ContextMenuItem onSelect={copyShas}>
            <Copy />
            Copy SHAs
          </ContextMenuItem>
        </ContextMenuContent>
      </ContextMenu>

      <RewordDialog
        open={pending === 'squash'}
        onOpenChange={(o) => !o && setPending(null)}
        title={`Combine ${plural(count, 'commit')} into one`}
        submitLabel="Combine commits"
        pendingLabel="Combining…"
        initialSummary={squashSummary}
        initialBody={squashBody}
        pending={m.squashCommits.isPending}
        onConfirm={(message) =>
          m.squashCommits.mutate({ shas, message }, { onSuccess: clearAndClose })
        }
      />

      <FormDialog
        open={pending === 'prefix'}
        onOpenChange={(o) => !o && setPending(null)}
        icon={<Pencil size={15} strokeWidth={1.9} />}
        title={`Add a prefix to ${plural(count, 'commit message')}`}
        submitLabel="Add prefix"
        pendingLabel="Updating…"
        canSubmit={prefix.trim() !== '' && !m.prefixCommits.isPending}
        pending={m.prefixCommits.isPending}
        onSubmit={() =>
          m.prefixCommits.mutate({ shas, prefix }, { onSuccess: clearAndClose })
        }
      >
        <div className="grid gap-1.5">
          <label className="text-2xs font-semibold text-sub">Prefix</label>
          <Input
            value={prefix}
            onChange={(e) => setPrefix(e.target.value)}
            placeholder="fixes: "
            className="h-auto bg-background py-1.5 font-mono text-xs"
            autoFocus
          />
          <p className="min-h-[15px] text-2xs leading-tight text-muted-foreground">
            {prefix.trim() !== '' ? (
              <>
                First one becomes:{' '}
                <span className="font-mono text-foreground">
                  {prefix}
                  {oldestFirst[0]?.summary}
                </span>
              </>
            ) : (
              'Added to the start of each selected message. Your files are not touched.'
            )}
          </p>
        </div>
        <p className="text-2xs leading-tight text-muted-foreground">
          The {plural(count, 'selected commit')} and everything above them get new IDs. You can
          undo this right after.
        </p>
      </FormDialog>

      <ConfirmDialog
        open={pending === 'drop'}
        onOpenChange={(o) => !o && setPending(null)}
        destructive
        title={`Drop ${plural(count, 'commit')}?`}
        description={
          <>
            This removes the {plural(count, 'selected commit')} from{' '}
            <span className="font-mono text-foreground">{branchName}</span> and replays the other
            commits in that stretch of history on top. If a later commit depended on one of these
            you may hit conflicts, in which case nothing is changed. You can undo this right after.
          </>
        }
        confirmLabel={`Drop ${plural(count, 'commit')}`}
        pending={m.dropCommits.isPending}
        pendingLabel="Dropping…"
        keepOpenOnConfirm
        onConfirm={() => m.dropCommits.mutate(shas, { onSuccess: clearAndClose })}
      />
    </>
  )
}
