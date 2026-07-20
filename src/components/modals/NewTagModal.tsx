import { useEffect, useMemo, useState } from 'react'
import { Tag } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Dialog, DialogContent, DialogHeader, DialogTitle } from '@/components/ui/dialog'
import { useTags } from '@/hooks/useGitQueries'
import { useGitMutations } from '@/hooks/useGitMutations'
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
    <Dialog open={open} onOpenChange={(o) => !o && closeModal()}>
      <DialogContent className="gap-0 p-0 sm:max-w-md" aria-describedby={undefined}>
        <DialogHeader className="border-b border-border px-4 pb-3 pt-4">
          <DialogTitle className="flex items-center gap-2 text-sm">
            <Tag size={15} strokeWidth={1.9} />
            New tag
          </DialogTitle>
        </DialogHeader>

        <div className="grid gap-3 px-4 py-4">
          <div className="grid gap-1.5">
            <label className="text-[11px] font-semibold text-sub">Tag name</label>
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
            <p className="min-h-[15px] text-[10.5px] leading-tight text-removed">{error ?? ''}</p>
          </div>

          <div className="grid gap-1.5">
            <label className="text-[11px] font-semibold text-sub">
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
            <p className="text-[10.5px] leading-tight text-muted-foreground">
              Add a message to make it an annotated tag; leave blank for a simple one.
            </p>
          </div>

          <p className="text-[10.5px] text-muted-foreground">
            Tags{' '}
            {targetSha ? (
              <>
                the commit <span className="font-mono text-sub">{targetSha.slice(0, 7)}</span>.
              </>
            ) : (
              'the latest commit on this branch.'
            )}
          </p>
        </div>

        <div className="flex justify-end gap-2 border-t border-border px-4 py-3">
          <Button variant="secondary" size="sm" onClick={closeModal}>
            Cancel
          </Button>
          <Button size="sm" disabled={!canCreate} onClick={create}>
            {m.createTag.isPending ? 'Creating…' : 'Create tag'}
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  )
}
