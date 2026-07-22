import { AlertTriangle, ArrowDown, Cloud } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { Dialog, DialogContent, DialogHeader, DialogTitle } from '@/components/ui/dialog'
import { useBranches } from '@/hooks/useGitQueries'
import { branchSync } from '@/lib/branchActions'
import { plural } from '@/lib/gitDisplay'
import { useGitMutations } from '@/hooks/useGitMutations'
import { useUiStore } from '@/stores/uiStore'
import { useActiveRepo } from '@/stores/workspaceStore'

/**
 * Shown when Push is pressed on a branch that is behind its upstream. A plain
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

  const repo = useActiveRepo()
  const branches = useBranches(repo?.id ?? null)
  const m = useGitMutations(repo?.id ?? null)

  const head = branches.data?.local.find((b) => b.is_head)
  const sync = head ? branchSync(head) : null
  const behind = sync?.behind ?? 0
  const ahead = sync?.ahead ?? 0
  const commits = (n: number) => plural(n, 'commit')

  const pending = m.pull.isPending || m.pushForce.isPending

  const getFirst = () => m.pull.mutate(undefined, { onSuccess: () => closeModal() })
  const forcePush = () => m.pushForce.mutate(undefined, { onSuccess: () => closeModal() })

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
                The cloud has {commits(behind)} that {head?.name ?? 'this branch'} doesn't
                {ahead > 0 ? `, and you have ${commits(ahead)} it doesn't` : ''}. A normal push
                would be turned down.
              </span>
            </span>
          </div>

          <ul className="grid gap-2 text-2xs leading-relaxed text-muted-foreground">
            <li>
              <span className="font-medium text-foreground">Get changes first</span> pulls the
              cloud's work into yours, then you can push. Nothing is lost.
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
            <ArrowDown size={13} /> {m.pull.isPending ? 'Getting…' : 'Get changes first'}
          </Button>
          <Button variant="destructive" size="sm" disabled={pending} onClick={forcePush}>
            {m.pushForce.isPending ? 'Force pushing…' : 'Force push'}
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  )
}
