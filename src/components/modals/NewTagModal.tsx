import { useEffect, useMemo, useState } from 'react'
import { Tag } from 'lucide-react'
import { Input } from '@/components/ui/input'
import { FormDialog } from '@/components/ui/form-dialog'
import { useTags } from '@/hooks/useGitQueries'
import { useGitMutations } from '@/hooks/useGitMutations'
import { shortSha } from '@/lib/gitDisplay'
import { refNameError } from '@/lib/refName'
import { useUiStore } from '@/stores/uiStore'
import { useActiveRepo } from '@/stores/workspaceStore'

export function NewTagModal() {
  const open = useUiStore((s) => s.activeModal === 'newTag')
  const closeModal = useUiStore((s) => s.closeModal)
  const targetSha = useUiStore((s) => s.tagTargetSha)

  const repo = useActiveRepo()
  const tags = useTags(repo?.id ?? null)
  const m = useGitMutations(repo?.id ?? null)

  const [name, setName] = useState('')
  const [message, setMessage] = useState('')

  useEffect(() => {
    if (open) {
      setName('')
      setMessage('')
    }
  }, [open])

  const existing = useMemo(
    () => new Set((tags.data ?? []).map((t) => t.name)),
    [tags.data]
  )

  const trimmed = name.trim()
  const error = refNameError(trimmed, [...existing], 'tag')

  const canCreate = trimmed !== '' && !error && !m.createTag.isPending

  const create = () => {
    if (!canCreate) return
    m.createTag.mutate(
      { name: trimmed, sha: targetSha ?? '', message: message.trim() },
      { onSuccess: () => closeModal() }
    )
  }

  return (
    <FormDialog
      open={open}
      onOpenChange={(o) => !o && closeModal()}
      icon={<Tag size={15} strokeWidth={1.9} />}
      title="New tag"
      submitLabel="Create tag"
      pendingLabel="Creating…"
      canSubmit={canCreate}
      pending={m.createTag.isPending}
      onSubmit={create}
    >
      <div className="grid gap-1.5">
        <label className="text-2xs font-semibold text-sub">Tag name</label>
        <Input
          value={name}
          onChange={(e) => setName(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === 'Enter') create()
          }}
          placeholder="v1.0.0"
          className="h-auto bg-background py-1.5 font-mono text-xs"
          autoFocus
        />
        <p className="min-h-[15px] text-2xs leading-tight text-removed">{error ?? ''}</p>
      </div>

      <div className="grid gap-1.5">
        <label className="text-2xs font-semibold text-sub">
          Message <span className="font-normal text-muted-foreground">(optional)</span>
        </label>
        <Input
          value={message}
          onChange={(e) => setMessage(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === 'Enter') create()
          }}
          placeholder="What this release is"
          className="h-auto bg-background py-1.5 text-xs"
        />
        <p className="text-2xs leading-tight text-muted-foreground">
          Add a message to make it an annotated tag; leave blank for a simple one.
        </p>
      </div>

      <p className="text-2xs text-muted-foreground">
        Tags{' '}
        {targetSha ? (
          <>
            the commit <span className="font-mono text-sub">{shortSha(targetSha)}</span>.
          </>
        ) : (
          'the latest commit on this branch.'
        )}
      </p>
    </FormDialog>
  )
}
