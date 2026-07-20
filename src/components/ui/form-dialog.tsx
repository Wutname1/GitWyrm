import type { ReactNode } from 'react'
import { Button } from '@/components/ui/button'
import { PendingIndicator } from '@/components/ui/pending-indicator'
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'

interface FormDialogProps {
  open: boolean
  onOpenChange: (open: boolean) => void
  /** Small icon beside the title. */
  icon?: ReactNode
  title: ReactNode
  children: ReactNode
  /** Extra footer content, placed left of Cancel. */
  footerExtra?: ReactNode
  cancelLabel?: string
  submitLabel: string
  /** Button text while the action runs. Falls back to `submitLabel`. */
  pendingLabel?: string
  /** Disables the submit button; the dialog decides what "ready" means. */
  canSubmit: boolean
  pending?: boolean
  destructive?: boolean
  onSubmit: () => void
}

/**
 * The shell every form dialog shares: header, body, and a Cancel/submit footer.
 *
 * It also owns the rule that a dialog cannot be dismissed while its action is
 * running -- Escape and outside-clicks are ignored until it finishes, so a
 * half-done operation can't be hidden from the user. That guard was previously
 * written out in each dialog, which is exactly the kind of thing that gets
 * fixed in one place and missed in the others.
 */
export function FormDialog({
  open,
  onOpenChange,
  icon,
  title,
  children,
  footerExtra,
  cancelLabel = 'Cancel',
  submitLabel,
  pendingLabel,
  canSubmit,
  pending = false,
  destructive = false,
  onSubmit,
}: FormDialogProps) {
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
            {icon}
            {title}
          </DialogTitle>
        </DialogHeader>

        <div className="grid gap-3 px-4 py-4">{children}</div>

        <DialogFooter className="flex items-center justify-end gap-2 border-t border-border px-4 py-3">
          {footerExtra}
          <Button
            variant="secondary"
            size="sm"
            disabled={pending}
            onClick={() => onOpenChange(false)}
          >
            {cancelLabel}
          </Button>
          <Button
            size="sm"
            variant={destructive ? 'destructive' : 'default'}
            disabled={!canSubmit || pending}
            aria-busy={pending || undefined}
            onClick={onSubmit}
          >
            {pending && <PendingIndicator />}
            {pending ? (pendingLabel ?? submitLabel) : submitLabel}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
