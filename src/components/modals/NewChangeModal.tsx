import { useEffect, useMemo, useState } from 'react'
import { FilePlus2 } from 'lucide-react'
import { toast } from 'sonner'
import { Input } from '@/components/ui/input'
import { Textarea } from '@/components/ui/textarea'
import { FormDialog } from '@/components/ui/form-dialog'
import { useOpenspecChanges, useOpenspecMutations } from '@/hooks/useOpenspec'
import { selectChangeEverywhere } from '@/lib/specSync'
import { useUiStore } from '@/stores/uiStore'
import { useActiveRepo } from '@/stores/workspaceStore'

/**
 * Preview of the folder name a title will become.
 *
 * Mirrors `sanitize_change_id` in src-tauri/src/openspec/write.rs, which is the
 * source of truth -- this only exists so the user sees the real name before
 * committing to it. The backend sanitizes again regardless, so a drift here
 * costs a surprising preview, never a wrong folder.
 */
function previewId(raw: string): string {
  const out: string[] = []
  let lastDash = true
  for (const ch of raw.trim()) {
    const c = ch.toLowerCase()
    if (/[a-z0-9]/.test(c)) {
      out.push(c)
      lastDash = false
    } else if (!lastDash) {
      out.push('-')
      lastDash = true
    }
  }
  return out.join('').replace(/-+$/, '')
}

/**
 * @param repoId Repository to create the change in. Omitted in the main window,
 * which reads the active repo from the store. The Spec Desk must pass its own:
 * that window never runs launch restore, so its store has no open repos and no
 * active one -- `useActiveRepo()` there is always null, and creating a change
 * would reach the backend with no repository at all.
 */
export function NewChangeModal({ repoId }: { repoId?: string }) {
  const open = useUiStore((s) => s.activeModal === 'newChange')
  const closeModal = useUiStore((s) => s.closeModal)

  const activeRepo = useActiveRepo()
  const targetRepoId = repoId ?? activeRepo?.id ?? null
  const changes = useOpenspecChanges(targetRepoId)
  const { scaffoldChange } = useOpenspecMutations(targetRepoId)

  const [title, setTitle] = useState('')
  const [why, setWhy] = useState('')

  useEffect(() => {
    if (open) {
      setTitle('')
      setWhy('')
    }
  }, [open])

  const id = previewId(title)
  const taken = useMemo(
    () => new Set((changes.data ?? []).map((c) => c.id)),
    [changes.data]
  )

  const error =
    title.trim() === ''
      ? null
      : id === ''
        ? 'Use at least one letter or number.'
        : taken.has(id)
          ? `There is already a change named ${id}.`
          : null

  const canCreate = targetRepoId != null && id !== '' && !error && !scaffoldChange.isPending

  const create = () => {
    if (!canCreate) return
    scaffoldChange.mutate(
      { name: title.trim(), description: why.trim() },
      {
        onSuccess: (result) => {
          // Select it so the card and the Desk land on what was just made.
          selectChangeEverywhere(result.id)
          closeModal()
          toast.success(`Started ${result.id}.`, {
            description: 'Write the proposal and add tasks in the Spec Desk.',
          })
        },
        onError: (e) => toast.error(String(e)),
      }
    )
  }

  return (
    <FormDialog
      open={open}
      onOpenChange={(o) => !o && closeModal()}
      icon={<FilePlus2 size={15} strokeWidth={1.9} />}
      title="Start a new change"
      submitLabel="Create it"
      pendingLabel="Creating…"
      canSubmit={canCreate}
      pending={scaffoldChange.isPending}
      onSubmit={create}
    >
      <div className="grid gap-1.5">
        <label className="text-2xs font-semibold text-sub">What are you changing?</label>
        <Input
          value={title}
          onChange={(e) => setTitle(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === 'Enter') create()
          }}
          placeholder="Add dark mode to the diff view"
          className="h-auto bg-background py-1.5 text-xs"
          autoFocus
        />
        <p className="min-h-[15px] text-2xs leading-tight">
          {error ? (
            <span className="text-removed">{error}</span>
          ) : id ? (
            <span className="text-muted-foreground">
              Saved as <span className="font-mono text-sub">openspec/changes/{id}</span>
            </span>
          ) : (
            ''
          )}
        </p>
      </div>

      <div className="grid gap-1.5">
        <label className="text-2xs font-semibold text-sub">
          Why does it matter? <span className="font-normal text-muted-foreground">(optional)</span>
        </label>
        <Textarea
          value={why}
          onChange={(e) => setWhy(e.target.value)}
          placeholder="The problem this solves."
          rows={3}
          className="bg-background px-2.5 py-2 text-xs"
        />
      </div>

      <p className="text-2xs text-muted-foreground">
        Creates a proposal and an empty task list. You fill them in from the Spec Desk.
      </p>
    </FormDialog>
  )
}
