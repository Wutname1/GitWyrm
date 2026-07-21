import { useEffect, useMemo, useState } from 'react'
import { Tag } from 'lucide-react'
import { Input } from '@/components/ui/input'
import { FormDialog } from '@/components/ui/form-dialog'
import { useTags } from '@/hooks/useGitQueries'
import { useGitMutations } from '@/hooks/useGitMutations'
import { useTagSync } from '@/hooks/useTagSync'
import { shortSha } from '@/lib/gitDisplay'
import { refNameError } from '@/lib/refName'
import { useUiStore } from '@/stores/uiStore'
import { useActiveRepo, useWorkspaceStore } from '@/stores/workspaceStore'

export function NewTagModal() {
  const open = useUiStore((s) => s.activeModal === 'newTag')
  const closeModal = useUiStore((s) => s.closeModal)
  const targetSha = useUiStore((s) => s.tagTargetSha)

  const repo = useActiveRepo()
  const tags = useTags(repo?.id ?? null)
  const m = useGitMutations(repo?.id ?? null)

  // The dialog is the only place that needs the host's name, and only while it
  // is open, so the remote lookup is gated on that.
  const { hostLabel, hasRemote } = useTagSync(repo?.id ?? null, open)

  const tagPushOnCreate = useWorkspaceStore((s) => s.tagPushOnCreate)
  const setTagPushOnCreate = useWorkspaceStore((s) => s.setTagPushOnCreate)

  const [name, setName] = useState('')
  const [message, setMessage] = useState('')
  const [push, setPush] = useState(false)

  useEffect(() => {
    if (open) {
      setName('')
      setMessage('')
      // Start from the remembered choice each time the dialog opens.
      setPush(tagPushOnCreate)
    }
  }, [open, tagPushOnCreate])

  const existing = useMemo(
    () => new Set((tags.data ?? []).map((t) => t.name)),
    [tags.data]
  )

  const trimmed = name.trim()
  const error = refNameError(trimmed, [...existing], 'tag')

  const canCreate = trimmed !== '' && !error && !m.createTag.isPending
  const sendIt = push && hasRemote

  const create = () => {
    if (!canCreate) return
    // Remember the checkbox so the next tag starts from the same choice.
    setTagPushOnCreate(sendIt)
    m.createTag.mutate(
      { name: trimmed, sha: targetSha ?? '', message: message.trim(), push: sendIt },
      { onSuccess: () => closeModal() }
    )
  }

  return (
    <FormDialog
      open={open}
      onOpenChange={(o) => !o && closeModal()}
      icon={<Tag size={15} strokeWidth={1.9} />}
      title="New tag"
      submitLabel={sendIt ? 'Create and send' : 'Create tag'}
      pendingLabel={sendIt ? 'Sending…' : 'Creating…'}
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

      {hasRemote ? (
        <label className="flex cursor-pointer items-center gap-2 text-xs text-sub">
          <input
            type="checkbox"
            checked={push}
            onChange={(e) => setPush(e.target.checked)}
            className="size-3.5 accent-[var(--gw-accent)]"
          />
          Also send it to {hostLabel} now
        </label>
      ) : (
        <p className="text-2xs text-muted-foreground">
          This tag stays on your computer. Add a remote to be able to send it.
        </p>
      )}

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
