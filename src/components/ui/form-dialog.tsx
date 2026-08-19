import { useRef, type KeyboardEvent, type ReactNode } from 'react'
import { cn } from '@/lib/utils'
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
  /** Disables the submit button; the dialog decides what "ready" means. Defaults to enabled. */
  canSubmit?: boolean
  pending?: boolean
  destructive?: boolean
  /** Width class, for dialogs whose body is more than a couple of fields. */
  widthClassName?: string
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
 *
 * Enter submits from anywhere in the dialog, so a confirmation like "Delete
 * branch?" answers to the key people already have their hand on. Textareas keep
 * Enter for newlines, and Enter on a focused button still presses that button --
 * otherwise Cancel would silently become Confirm.
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
  canSubmit = true,
  pending = false,
  destructive = false,
  widthClassName = 'sm:max-w-md',
  onSubmit,
}: FormDialogProps) {
  const bodyRef = useRef<HTMLDivElement>(null)
  const submitRef = useRef<HTMLButtonElement>(null)

  const handleKeyDown = (e: KeyboardEvent<HTMLDivElement>) => {
    // `repeat` guards against a held key firing the action many times before
    // the pending state has a chance to re-render and disable it.
    if (e.key !== 'Enter' || e.shiftKey || e.altKey || e.repeat || e.isDefaultPrevented()) return

    const target = e.target as HTMLElement | null
    if (!target) return
    // Textareas need Enter for newlines; buttons need it to press themselves.
    if (target.closest('textarea, [contenteditable="true"], button, a[href]')) return

    if (!canSubmit || pending) return
    e.preventDefault()
    onSubmit()
  }

  /**
   * Confirmations have nothing else worth focusing, so point at the submit
   * button: it shows a focus ring, which is the only cue that Enter is live.
   * Dialogs with a field keep their own `autoFocus` -- typing comes first there.
   */
  const focusSubmitIfNoField = (e: Event) => {
    const body = bodyRef.current
    if (body?.querySelector('input:not([type="checkbox"]):not([type="radio"]), textarea, select')) return
    const submit = submitRef.current
    // A disabled submit can't hold focus; let Radix do its usual thing instead.
    if (!submit || submit.disabled) return
    // Radix would otherwise focus the first tabbable node -- the X close button.
    e.preventDefault()
    submit.focus()
  }

  return (
    <Dialog
      open={open}
      onOpenChange={(next) => {
        if (!next && pending) return
        onOpenChange(next)
      }}
    >
      <DialogContent
        className={cn('gap-0 p-0', widthClassName)}
        aria-describedby={undefined}
        onKeyDown={handleKeyDown}
        onOpenAutoFocus={focusSubmitIfNoField}
      >
        <DialogHeader className="border-b border-border px-4 pb-3 pt-4">
          <DialogTitle className="flex items-center gap-2 text-sm">
            {icon}
            {title}
          </DialogTitle>
        </DialogHeader>

        <div ref={bodyRef} className="grid gap-3 px-4 py-4">
          {children}
        </div>

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
            ref={submitRef}
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
