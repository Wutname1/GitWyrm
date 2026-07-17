import { useEffect, useState } from 'react'
import { AlertTriangle } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'

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
}: ConfirmDialogProps) {
  const [typed, setTyped] = useState('')

  useEffect(() => {
    if (open) setTyped('')
  }, [open])

  const ready = !confirmPhrase || typed.trim() === confirmPhrase

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="gap-0 p-0 sm:max-w-md" aria-describedby={undefined}>
        <DialogHeader className="border-b border-border px-4 pb-3 pt-4">
          <DialogTitle className="flex items-center gap-2 text-sm">
            {destructive && <AlertTriangle size={15} className="text-removed" strokeWidth={2} />}
            {title}
          </DialogTitle>
        </DialogHeader>

        <div className="grid gap-3 px-4 py-4">
          <DialogDescription className="text-[12px] leading-relaxed text-sub">
            {description}
          </DialogDescription>
          {confirmPhrase && (
            <div className="grid gap-1.5">
              <label className="text-[11px] text-muted-foreground">
                Type <span className="font-mono text-foreground">{confirmPhrase}</span> to confirm
              </label>
              <Input
                value={typed}
                onChange={(e) => setTyped(e.target.value)}
                placeholder={confirmPhrase}
                className="h-auto bg-background py-1.5 font-mono text-xs"
                autoFocus
              />
            </div>
          )}
        </div>

        <DialogFooter className="flex justify-end gap-2 border-t border-border px-4 py-3">
          <Button variant="secondary" size="sm" onClick={() => onOpenChange(false)}>
            Cancel
          </Button>
          <Button
            size="sm"
            variant={destructive ? 'destructive' : 'default'}
            disabled={!ready}
            onClick={() => {
              onConfirm()
              onOpenChange(false)
            }}
          >
            {confirmLabel}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
