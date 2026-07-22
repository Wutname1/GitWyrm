import { AlertTriangle } from 'lucide-react'
import { DialogDescription } from '@/components/ui/dialog'
import { FormDialog } from '@/components/ui/form-dialog'

interface ConfirmDialogProps {
  open: boolean
  onOpenChange: (open: boolean) => void
  title: string
  description: React.ReactNode
  confirmLabel: string
  destructive?: boolean
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
  onConfirm,
  pending = false,
  pendingLabel,
  keepOpenOnConfirm = false,
}: ConfirmDialogProps) {
  return (
    <FormDialog
      open={open}
      onOpenChange={onOpenChange}
      icon={destructive ? <AlertTriangle size={15} className="text-removed" strokeWidth={2} /> : undefined}
      title={title}
      submitLabel={confirmLabel}
      pendingLabel={pendingLabel ?? 'Working…'}
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
    </FormDialog>
  )
}
