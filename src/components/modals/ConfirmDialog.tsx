import { useEffect, useState } from 'react'
import { AlertTriangle } from 'lucide-react'
import { Input } from '@/components/ui/input'
import { DialogDescription } from '@/components/ui/dialog'
import { FormDialog } from '@/components/ui/form-dialog'

interface ConfirmDialogProps {
  open: boolean
  onOpenChange: (open: boolean) => void
  title: string
  description: React.ReactNode
  confirmLabel: string
  destructive?: boolean
  /** When set, the confirm button stays disabled until the user types this. */
  confirmPhrase?: string
  onConfirm: () => void
  pending?: boolean
  pendingLabel?: string
  keepOpenOnConfirm?: boolean
}

export function ConfirmDialog({
  open,
  onOpenChange,
  title,
  description,
  confirmLabel,
  destructive,
  confirmPhrase,
  onConfirm,
  pending = false,
  pendingLabel,
  keepOpenOnConfirm = false,
}: ConfirmDialogProps) {
  const [typed, setTyped] = useState('')

  useEffect(() => {
    if (open) setTyped('')
  }, [open])

  const ready = !confirmPhrase || typed.trim() === confirmPhrase

  return (
    <FormDialog
      open={open}
      onOpenChange={onOpenChange}
      icon={destructive ? <AlertTriangle size={15} className="text-removed" strokeWidth={2} /> : undefined}
      title={title}
      submitLabel={confirmLabel}
      pendingLabel={pendingLabel ?? 'Working…'}
      canSubmit={ready}
      pending={pending}
      destructive={destructive}
      onSubmit={() => {
        onConfirm()
        if (!keepOpenOnConfirm) onOpenChange(false)
      }}
    >
      <DialogDescription className="text-xs leading-relaxed text-sub">
        {description}
      </DialogDescription>
      {confirmPhrase && (
        <div className="grid gap-1.5">
          <label className="text-2xs text-muted-foreground">
            Type <span className="font-mono text-foreground">{confirmPhrase}</span> to confirm
          </label>
          <Input
            value={typed}
            onChange={(e) => setTyped(e.target.value)}
            placeholder={confirmPhrase}
            className="h-auto bg-background py-1.5 font-mono text-xs"
            autoFocus
            disabled={pending}
          />
        </div>
      )}
    </FormDialog>
  )
}
