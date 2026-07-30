import { useState } from 'react'
import { Archive, CheckCircle2, ClipboardCopy, ExternalLink, Hash } from 'lucide-react'
import type { SpecChange } from '@/lib/bindings'
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuLabel,
  ContextMenuSeparator,
  ContextMenuTrigger,
} from '@/components/ui/context-menu'
import { PendingMenuItem } from '@/components/ui/pending-menu-item'
import { ConfirmDialog } from '@/components/modals/ConfirmDialog'
import { nextTask, useOpenspecMutations } from '@/hooks/useOpenspec'
import { copyTaskHandoff } from '@/lib/specHandoff'
import { copyToClipboard } from '@/lib/clipboard'
import { openSpecDesk } from '@/lib/specDesk'
import { reportCliOutcome } from '@/lib/specCliOutcome'
import { selectChangeEverywhere } from '@/lib/specSync'

interface SpecContextMenuProps {
  change: SpecChange
  repoId: string
  children: React.ReactNode
}

/**
 * Right-click actions for one change, shared by the sidebar rows and the Desk's
 * changes list so both lists answer a right-click the same way.
 *
 * Items that cannot apply are left out rather than greyed: a change with open
 * tasks simply has no Archive item. A disabled row invites a click and then
 * explains nothing, which is the opposite of what a menu is for.
 */
export function SpecContextMenu({ change, repoId, children }: SpecContextMenuProps) {
  const [confirmArchive, setConfirmArchive] = useState(false)
  const { validateChange, archiveChange } = useOpenspecMutations(repoId)

  const task = nextTask(change)
  // A draft has no tasks at all, so "everything is done" would be a lie.
  const allDone = !task && !change.progress.is_draft

  const validate = () => {
    validateChange.mutate(change.id, {
      onSuccess: (outcome) =>
        reportCliOutcome(outcome, {
          ok: `${change.id} looks good.`,
          failed: `${change.id} has problems to fix.`,
        }),
    })
  }

  const archive = () => {
    archiveChange.mutate(change.id, {
      onSuccess: (outcome) =>
        reportCliOutcome(outcome, {
          ok: `Archived ${change.id}.`,
          failed: `Could not archive ${change.id}.`,
        }),
    })
  }

  return (
    <>
      {/* Right-click does not fire onClick, so without this the menu would act
          on one change while the card and Desk still show another. onOpenChange
          rather than an onContextMenu prop on the trigger: `asChild` merges
          props onto the child, and Radix owns onContextMenu there. */}
      <ContextMenu
        onOpenChange={(open) => {
          if (open) selectChangeEverywhere(change.id)
        }}
      >
        <ContextMenuTrigger asChild>{children}</ContextMenuTrigger>
        <ContextMenuContent className="w-60">
          <ContextMenuLabel className="overflow-hidden text-ellipsis whitespace-nowrap font-mono text-2xs text-sub">
            {change.id}
          </ContextMenuLabel>
          <ContextMenuSeparator />

          <ContextMenuItem onSelect={() => void openSpecDesk(repoId, change.id)}>
            <ExternalLink />
            Open in Spec Desk
          </ContextMenuItem>
          {!change.progress.is_draft && (
            <ContextMenuItem onSelect={() => void copyTaskHandoff(change, task)}>
              <ClipboardCopy />
              {allDone ? 'Copy review handoff' : 'Copy next-task handoff'}
            </ContextMenuItem>
          )}
          <ContextMenuItem
            onSelect={() => void copyToClipboard(change.id, `Copied ${change.id}`)}
          >
            <Hash />
            Copy change name
          </ContextMenuItem>

          <ContextMenuSeparator />
          <PendingMenuItem
            icon={<CheckCircle2 />}
            label="Check for problems"
            pendingLabel="Checking…"
            pending={validateChange.isPending}
            onRun={validate}
          />
          {allDone && (
            <ContextMenuItem onSelect={() => setConfirmArchive(true)}>
              <Archive />
              Archive this change…
            </ContextMenuItem>
          )}
        </ContextMenuContent>
      </ContextMenu>

      <ConfirmDialog
        open={confirmArchive}
        onOpenChange={setConfirmArchive}
        title={`Archive ${change.id}?`}
        description={
          <>
            Every task is done. Archiving folds this change into your specs and moves it
            out of the active list. It stays on disk, under{' '}
            <span className="font-mono">openspec/changes/archive</span>.
          </>
        }
        confirmLabel="Archive it"
        pending={archiveChange.isPending}
        pendingLabel="Archiving…"
        onConfirm={archive}
      />
    </>
  )
}
