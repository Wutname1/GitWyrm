import { AlertTriangle, ArrowDown, Cloud } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { Dialog, DialogContent, DialogHeader, DialogTitle } from '@/components/ui/dialog'
import { useBranches } from '@/hooks/useGitQueries'
import { branchSync } from '@/lib/branchActions'
import { pullNeedsChoice } from '@/lib/syncPreview'
import { plural } from '@/lib/gitDisplay'
import { useGitMutations } from '@/hooks/useGitMutations'
import { useUiStore } from '@/stores/uiStore'
import { useActiveRepo } from '@/stores/workspaceStore'

/**
 * Shown when Push is pressed on a branch that is behind its upstream, from the
 * toolbar (the checked-out branch) or from any branch's own menu. A plain
 * push would be refused as non-fast-forward, so instead of firing one that we
 * know fails, the user picks up front: get the cloud's changes first (safe), or
 * force push to replace the cloud's history with theirs.
 *
 * "Force push" is used verbatim -- the word makes the forcefulness clear rather
 * than dressing it up. A force push can still be turned down by branch
 * protection; that rejection is surfaced by the shared error classifier.
 */
export function PushChoiceModal() {
  const open = useUiStore((s) => s.activeModal === 'push-choice')
  const closeModal = useUiStore((s) => s.closeModal)
  const openRemoteSync = useUiStore((s) => s.openRemoteSync)
  const branchName = useUiStore((s) => s.pushChoiceBranch)

  const repo = useActiveRepo()
  const branches = useBranches(repo?.id ?? null)
  const m = useGitMutations(repo?.id ?? null)

  // Opened from a branch's own menu it is about that branch; from the toolbar,
  // the checked-out one.
  const target = branches.data?.local.find((b) => (branchName ? b.name === branchName : b.is_head))
  const sync = target ? branchSync(target) : null
  const behind = sync?.behind ?? 0
  const ahead = sync?.ahead ?? 0
  const commits = (n: number) => plural(n, 'commit')

  const pending = m.pull.isPending || m.pullBranch.isPending || m.pushForce.isPending || m.pushBranchForce.isPending

  // With work on both sides, "get first" would blend via a merge commit chosen
  // for the user. Hand that to the sync modal, which offers blend / stack /
  // replace and draws the result. A pure catch-up still pulls directly.
  const canChooseSync = pullNeedsChoice({ upstream: target?.upstream, ahead, behind })
  const getFirst = () => {
    if (canChooseSync) {
      closeModal()
      openRemoteSync(target!.upstream!, target!.name)
      return
    }
    if (target && !target.is_head) {
      m.pullBranch.mutate(target.name, { onSuccess: () => closeModal() })
      return
    }
    m.pull.mutate(undefined, { onSuccess: () => closeModal() })
  }
  const forcePush = () => {
    if (!target) return
    if (target.is_head) m.pushForce.mutate(undefined, { onSuccess: () => closeModal() })
    else m.pushBranchForce.mutate(target.name, { onSuccess: () => closeModal() })
  }

  return (
    <Dialog open={open} onOpenChange={(o) => !o && closeModal()}>
      <DialogContent className="gap-0 p-0 sm:max-w-md" aria-describedby={undefined}>
        <DialogHeader className="border-b border-border px-4 pb-3 pt-4">
          <DialogTitle className="flex items-center gap-2 text-sm">
            <Cloud size={15} strokeWidth={1.9} />
            The cloud has newer changes
          </DialogTitle>
        </DialogHeader>

        <div className="grid gap-3 px-4 py-4">
          <div className="rounded-md border border-border bg-panel2 px-3 py-2 text-xs leading-relaxed text-sub">
            <span className="flex items-start gap-1.5 text-modified">
              <AlertTriangle size={13} className="mt-[1px] flex-none" />
              <span>
                The cloud has {commits(behind)} that {target?.name ?? 'this branch'} doesn't
                {ahead > 0 ? `, and you have ${commits(ahead)} it doesn't` : ''}. A normal push
                would be turned down.
              </span>
            </span>
          </div>

          <ul className="grid gap-2 text-2xs leading-relaxed text-muted-foreground">
            <li>
              <span className="font-medium text-foreground">
                {canChooseSync ? 'Choose how to combine' : 'Get changes first'}
              </span>{' '}
              {canChooseSync
                ? "lets you pick how the two sets of changes come together, then you can push. Nothing is lost."
                : "pulls the cloud's work into yours, then you can push. Nothing is lost."}
            </li>
            <li>
              <span className="font-medium text-foreground">Force push</span> replaces the cloud's
              history with what you have now. The cloud's {behind === 1 ? 'change' : 'changes'} will
              be gone.
            </li>
          </ul>
        </div>

        <div className="flex justify-end gap-2 border-t border-border px-4 py-3">
          <Button variant="secondary" size="sm" disabled={pending} onClick={closeModal}>
            Cancel
          </Button>
          <Button variant="secondary" size="sm" disabled={pending} onClick={getFirst}>
            <ArrowDown size={13} />{' '}
            {m.pull.isPending || m.pullBranch.isPending
              ? 'Getting…'
              : canChooseSync
                ? 'Choose how to combine'
                : 'Get changes first'}
          </Button>
          <Button variant="destructive" size="sm" disabled={pending || !target} onClick={forcePush}>
            {m.pushForce.isPending || m.pushBranchForce.isPending ? 'Force pushing…' : 'Force push'}
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  )
}
