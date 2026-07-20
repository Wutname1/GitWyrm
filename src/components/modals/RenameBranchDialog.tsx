import { useEffect, useState } from 'react'
import { PenLine } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { PendingIndicator } from '@/components/ui/pending-indicator'
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'

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
  const taken = !unchanged && existingNames.includes(trimmed)
  // git rejects these outright; catching them here explains why rather than
  // surfacing a raw git error after the fact.
  const malformed = /[~^:?*[\]\\]|\.\.|@\{|^\/|\/$|\/\/|^\.|\.$/.test(trimmed)
  const ready = trimmed !== '' && !taken && !malformed && !unchanged && !pending

  const submit = () => {
    if (!ready) return
    onConfirm(trimmed)
  }

  return (
    <Dialog
      open={open}
      onOpenChange={(next) => {
        if (!next && pending) return
        onOpenChange(next)
      }}
    >
      <DialogContent className="gap-0 p-0 sm:max-w-md" aria-describedby={undefined}>
        <DialogHeader className="border-b border-border px-4 pb-3 pt-4">
          <DialogTitle className="flex items-center gap-2 text-sm">
            <PenLine size={15} strokeWidth={1.9} />
            Rename branch
          </DialogTitle>
        </DialogHeader>

        <div className="grid gap-3 px-4 py-4">
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

          {taken && (
            <p className="text-[10.5px] text-destructive">
              There's already a branch called{' '}
              <span className="font-mono">{trimmed}</span>.
            </p>
          )}
          {malformed && !taken && (
            <p className="text-[10.5px] text-destructive">
              That name has characters git won't accept. Try letters, numbers, dashes and slashes.
            </p>
          )}
          {hasUpstream && !taken && !malformed && (
            <p className="text-[10.5px] text-muted-foreground">
              This renames your copy only. The branch on the remote keeps its old name until you
              send this one.
            </p>
          )}
        </div>

        <DialogFooter className="flex justify-end gap-2 border-t border-border px-4 py-3">
          <Button variant="secondary" size="sm" disabled={pending} onClick={() => onOpenChange(false)}>
            Cancel
          </Button>
          <Button size="sm" disabled={!ready} aria-busy={pending || undefined} onClick={submit}>
            {pending && <PendingIndicator />}
            {pending ? 'Renaming…' : 'Rename branch'}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
