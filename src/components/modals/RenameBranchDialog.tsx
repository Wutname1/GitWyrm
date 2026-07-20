import { useEffect, useState } from 'react'
import { PenLine } from 'lucide-react'
import { refNameError } from '@/lib/refName'
import { Input } from '@/components/ui/input'
import { FormDialog } from '@/components/ui/form-dialog'

interface RenameBranchDialogProps {
  open: boolean
  onOpenChange: (open: boolean) => void
  /** The branch being renamed; prefills the field. */
  currentName: string
  /** Names already in use, so a collision is caught before submitting. */
  existingNames: string[]
  /** Whether the branch has been sent to a remote, which the rename won't touch. */
  hasUpstream: boolean
  pending: boolean
  onConfirm: (newName: string) => void
}

/** Rename a local branch. Safe on the branch you are on; the remote copy is
 *  left alone, which the dialog says outright so nobody expects otherwise. */
export function RenameBranchDialog({
  open,
  onOpenChange,
  currentName,
  existingNames,
  hasUpstream,
  pending,
  onConfirm,
}: RenameBranchDialogProps) {
  const [name, setName] = useState(currentName)

  useEffect(() => {
    if (open) setName(currentName)
  }, [open, currentName])

  const trimmed = name.trim()
  const unchanged = trimmed === currentName
  // Renaming to the current name is a no-op, not a collision, so the branch's
  // own name is excluded before the taken check.
  const error = unchanged
    ? null
    : refNameError(trimmed, existingNames.filter((n) => n !== currentName), 'branch')
  const ready = trimmed !== '' && !error && !unchanged && !pending

  const submit = () => {
    if (!ready) return
    onConfirm(trimmed)
  }

  return (
    <FormDialog
      open={open}
      onOpenChange={onOpenChange}
      icon={<PenLine size={15} strokeWidth={1.9} />}
      title="Rename branch"
      submitLabel="Rename branch"
      pendingLabel="Renaming…"
      canSubmit={ready}
      pending={pending}
      onSubmit={submit}
    >
      <div className="grid gap-1.5">
        <label className="text-[11px] font-semibold text-sub">New name</label>
        <Input
          value={name}
          onChange={(e) => setName(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === 'Enter') submit()
          }}
          placeholder={currentName}
          className="h-auto bg-background py-1.5 font-mono text-xs"
          autoFocus
        />
      </div>

      {error && <p className="text-[10.5px] text-destructive">{error}</p>}
      {hasUpstream && !error && (
        <p className="text-[10.5px] text-muted-foreground">
          This renames your copy only. The branch on the remote keeps its old name until you send
          this one.
        </p>
      )}
    </FormDialog>
  )
}
