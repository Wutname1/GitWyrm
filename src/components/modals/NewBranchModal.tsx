import { useEffect, useMemo, useState } from 'react'
import { GitBranch } from 'lucide-react'
import { Input } from '@/components/ui/input'
import { FormDialog } from '@/components/ui/form-dialog'
import { useBranches } from '@/hooks/useGitQueries'
import { useGitMutations } from '@/hooks/useGitMutations'
import { refNameError } from '@/lib/refName'
import { useUiStore } from '@/stores/uiStore'
import { useActiveRepo } from '@/stores/workspaceStore'

export function NewBranchModal() {
  const open = useUiStore((s) => s.activeModal === 'newBranch')
  const closeModal = useUiStore((s) => s.closeModal)
  const targetSha = useUiStore((s) => s.branchTargetSha)

  const repo = useActiveRepo()
  const branches = useBranches(repo?.id ?? null)
  const m = useGitMutations(repo?.id ?? null)

  const [name, setName] = useState('')
  const [checkout, setCheckout] = useState(true)

  useEffect(() => {
    if (open) {
      setName('')
      setCheckout(true)
    }
  }, [open])

  const current = branches.data?.local.find((b) => b.is_head)?.name ?? ''
  const existing = useMemo(
    () => new Set((branches.data?.local ?? []).map((b) => b.name)),
    [branches.data]
  )

  const trimmed = name.trim()
  const error = refNameError(trimmed, [...existing], 'branch')

  const canCreate = trimmed !== '' && !error && !m.createBranch.isPending

  const create = () => {
    if (!canCreate) return
    m.createBranch.mutate(
      { name: trimmed, sha: targetSha ?? undefined, checkout },
      { onSuccess: () => closeModal() }
    )
  }

  return (
    <FormDialog
      open={open}
      onOpenChange={(o) => !o && closeModal()}
      icon={<GitBranch size={15} strokeWidth={1.9} />}
      title="New branch"
      submitLabel="Create branch"
      pendingLabel="Creating…"
      canSubmit={canCreate}
      pending={m.createBranch.isPending}
      onSubmit={create}
    >
      <div className="grid gap-1.5">
        <label className="text-[11px] font-semibold text-sub">Branch name</label>
        <Input
          value={name}
          onChange={(e) => setName(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === 'Enter') create()
          }}
          placeholder="my-feature"
          className="h-auto bg-background py-1.5 font-mono text-xs"
          autoFocus
        />
        <p className="min-h-[15px] text-[10.5px] leading-tight text-removed">{error ?? ''}</p>
      </div>

      <label className="flex cursor-pointer items-center gap-2 text-[11.5px] text-sub">
        <input
          type="checkbox"
          checked={checkout}
          onChange={(e) => setCheckout(e.target.checked)}
          className="size-3.5 accent-[var(--gw-accent)]"
        />
        Switch to it after creating
      </label>

      {targetSha ? (
        <p className="text-[10.5px] text-muted-foreground">
          Branches off commit <span className="font-mono text-sub">{shortSha(targetSha)}</span>.
        </p>
      ) : (
        current && (
          <p className="text-[10.5px] text-muted-foreground">
            Branches off <span className="font-mono text-sub">{current}</span>.
          </p>
        )
      )}
    </FormDialog>
  )
}
