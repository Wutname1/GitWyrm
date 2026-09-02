import { useState } from 'react'
import { Archive, Trash2 } from 'lucide-react'
import { toast } from 'sonner'
import type { SpecChange } from '@/lib/bindings'
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip'
import { ConfirmDialog } from '@/components/modals/ConfirmDialog'
import { useOpenspecMutations } from '@/hooks/useOpenspec'
import { reportCliOutcome } from '@/lib/specCliOutcome'
import { useWorkspaceStore } from '@/stores/workspaceStore'
import { cn } from '@/lib/utils'

/**
 * One icon button in the row's action strip.
 *
 * `stopPropagation` on the click is load-bearing: these sit inside the row's
 * <button>, so without it archiving a change would also select it, and the Desk
 * would jump to a change that is about to disappear.
 */
function RowAction({
  label,
  icon,
  destructive,
  disabled,
  onRun,
}: {
  label: string
  icon: React.ReactNode
  destructive?: boolean
  disabled?: boolean
  onRun: () => void
}) {
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <span
          role="button"
          tabIndex={disabled ? -1 : 0}
          aria-label={label}
          aria-disabled={disabled || undefined}
          onClick={(e) => {
            e.stopPropagation()
            e.preventDefault()
            if (!disabled) onRun()
          }}
          onKeyDown={(e) => {
            if (e.key !== 'Enter' && e.key !== ' ') return
            e.stopPropagation()
            e.preventDefault()
            if (!disabled) onRun()
          }}
          className={cn(
            'flex h-5 w-5 flex-none items-center justify-center rounded transition-colors',
            disabled
              ? 'cursor-default text-muted-foreground/40'
              : destructive
                ? 'cursor-pointer text-muted-foreground hover:bg-removed/15 hover:text-removed'
                : 'cursor-pointer text-muted-foreground hover:bg-panel3 hover:text-foreground'
          )}
        >
          {icon}
        </span>
      </TooltipTrigger>
      <TooltipContent side="top">{label}</TooltipContent>
    </Tooltip>
  )
}

/**
 * The "Don't ask again" opt-out shown inside both confirmations.
 *
 * Ticking it does nothing on its own -- the preference is only saved if the user
 * goes on to confirm. Ticking the box and then cancelling means "no", so it must
 * not quietly turn the prompt off on the way out.
 */
function DontAskAgain({
  checked,
  onChange,
  note,
}: {
  checked: boolean
  onChange: (checked: boolean) => void
  note: string
}) {
  return (
    <label className="flex cursor-pointer items-start gap-2 text-xs text-sub">
      <input
        type="checkbox"
        checked={checked}
        onChange={(e) => onChange(e.target.checked)}
        className="mt-0.5 size-3.5 accent-[var(--gw-accent)]"
      />
      <span>
        Don't ask me again
        <span className="block text-2xs text-muted-foreground">{note}</span>
      </span>
    </label>
  )
}

/**
 * Archive and Delete, on the right edge of a change row.
 *
 * The pair is deliberate: archiving keeps the work (its deltas fold into the
 * specs library), deleting throws it away. Both ask first, and each dialog says
 * which of those two things is about to happen so the icons never have to carry
 * that meaning alone. Each confirmation carries its own "Don't ask me again",
 * remembered separately -- waving through archiving must not also wave through
 * deleting.
 *
 * Archive is shown greyed rather than hidden when tasks are still open, because
 * unlike the right-click menu this strip is always visible -- a button that
 * appears and disappears as tasks get ticked is harder to read than one that is
 * plainly not available yet. Its tooltip says why.
 *
 * Nested inside the row <button>, so these are `role="button"` spans: a real
 * <button> inside a <button> is invalid HTML and browsers drop it.
 */
export function SpecRowActions({
  change,
  repoId,
  className,
}: {
  change: SpecChange
  repoId: string
  className?: string
}) {
  const [confirm, setConfirm] = useState<'archive' | 'delete' | null>(null)
  // Reset every time a dialog opens, so a box ticked and then cancelled does not
  // come back pre-ticked next time.
  const [dontAsk, setDontAsk] = useState(false)
  const { archiveChange, deleteChange } = useOpenspecMutations(repoId)

  const skipArchivePrompt = useWorkspaceStore((s) => s.openspecArchiveWithoutAsking)
  const skipDeletePrompt = useWorkspaceStore((s) => s.openspecDeleteWithoutAsking)
  const setSkipArchivePrompt = useWorkspaceStore((s) => s.setOpenspecArchiveWithoutAsking)
  const setSkipDeletePrompt = useWorkspaceStore((s) => s.setOpenspecDeleteWithoutAsking)

  const openTasks = change.progress.total - change.progress.done
  // A draft has no tasks at all, so "everything is done" would be a lie.
  const canArchive = openTasks === 0 && !change.progress.is_draft

  const archive = () => {
    archiveChange.mutate(
      { changeId: change.id },
      {
        onSuccess: (attempt) => {
          // A row has no room for an explanation and an override, so a blocked
          // archive points at the Desk's rail, where both live. It never forces
          // past the readiness check from here.
          if (attempt.kind === 'blocked') {
            toast.info(attempt.summary, {
              description: 'Open the Spec Desk to see what happens if you archive anyway.',
            })
            return
          }
          // The tool exited cleanly but nothing moved. Saying "Archived" here is
          // what made this look done while the change stayed put.
          if (attempt.kind === 'changedNothing') {
            // The tool's own words are the only real explanation, and they were
            // already being carried here and thrown away. Pointing at the Spec
            // Desk instead was a dead end: that panel holds the output in local
            // state, so opening it afterwards shows nothing at all.
            const said = attempt.output?.trim()
            toast.error(`${change.id} was not archived.`, {
              description: said
                ? `${attempt.diagnosis.summary} The OpenSpec tool said: ${said}`
                : `${attempt.diagnosis.summary} The OpenSpec tool exited without saying why - see the log for details.`,
            })
            return
          }
          reportCliOutcome(attempt.outcome, {
            ok: `Archived ${change.id}.`,
            failed: `Could not archive ${change.id}.`,
          })
        },
      }
    )
  }

  const remove = () => {
    deleteChange.mutate(
      { changeId: change.id },
      {
        onSuccess: () => toast.success(`Deleted ${change.id}.`),
        onError: (e) =>
          toast.error(`Could not delete ${change.id}.`, { description: String(e) }),
      }
    )
  }

  /** Opens the confirmation, or runs straight away if it was waved through. */
  const request = (which: 'archive' | 'delete') => {
    const skip = which === 'archive' ? skipArchivePrompt : skipDeletePrompt
    if (skip) {
      if (which === 'archive') archive()
      else remove()
      return
    }
    setDontAsk(false)
    setConfirm(which)
  }

  /** Saves the opt-out (only on a real confirm) and runs the action. */
  const confirmed = (which: 'archive' | 'delete') => {
    if (dontAsk) {
      if (which === 'archive') setSkipArchivePrompt(true)
      else setSkipDeletePrompt(true)
    }
    if (which === 'archive') archive()
    else remove()
  }

  return (
    <>
      <span className={cn('flex items-center gap-0.5', className)}>
        <RowAction
          label={
            canArchive
              ? 'Archive this change'
              : change.progress.is_draft
                ? 'Still a draft, so there is nothing to archive yet'
                : `${openTasks} ${openTasks === 1 ? 'task is' : 'tasks are'} still open`
          }
          icon={<Archive size={12} strokeWidth={2} />}
          disabled={!canArchive}
          onRun={() => request('archive')}
        />
        <RowAction
          label="Delete this change"
          icon={<Trash2 size={12} strokeWidth={2} />}
          destructive
          onRun={() => request('delete')}
        />
      </span>

      <ConfirmDialog
        open={confirm === 'archive'}
        onOpenChange={(open) => !open && setConfirm(null)}
        title={`Archive ${change.id}?`}
        description={
          <>
            Every task is done. Archiving folds this change into your specs and moves it
            out of the active list. It stays on disk, under{' '}
            <span className="font-mono">openspec/changes/archive</span>.
          </>
        }
        extra={
          <DontAskAgain
            checked={dontAsk}
            onChange={setDontAsk}
            note="Archive from this button without asking. The Spec Desk settings put this back."
          />
        }
        confirmLabel="Archive it"
        pending={archiveChange.isPending}
        pendingLabel="Archiving…"
        onConfirm={() => confirmed('archive')}
      />

      <ConfirmDialog
        open={confirm === 'delete'}
        onOpenChange={(open) => !open && setConfirm(null)}
        destructive
        title={`Delete ${change.id}?`}
        description={
          <>
            This throws the change away: its proposal, tasks, and specs are removed from{' '}
            <span className="font-mono">openspec/changes/{change.id}</span>. Nothing is
            folded into your specs library.
            {openTasks > 0 && (
              <>
                {' '}
                {openTasks} {openTasks === 1 ? 'task is' : 'tasks are'} still open.
              </>
            )}{' '}
            If you have committed this change before, you can still recover it from your
            history.
          </>
        }
        extra={
          <DontAskAgain
            checked={dontAsk}
            onChange={setDontAsk}
            note="Delete from this button without asking. The Spec Desk settings put this back."
          />
        }
        confirmLabel="Delete it"
        pending={deleteChange.isPending}
        pendingLabel="Deleting…"
        onConfirm={() => confirmed('delete')}
      />
    </>
  )
}
