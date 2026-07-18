import { useEffect, useState } from 'react'
import { Pencil } from 'lucide-react'
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

interface RewordDialogProps {
  open: boolean
  onOpenChange: (open: boolean) => void
  /** Current summary + body, used to prefill the editor. */
  initialSummary: string
  initialBody: string
  pending: boolean
  onConfirm: (message: string) => void
}

/** Edit a commit's message. Summary on one line, an optional body below; they
 *  are joined the same way git stores them (summary, blank line, body). */
export function RewordDialog({
  open,
  onOpenChange,
  initialSummary,
  initialBody,
  pending,
  onConfirm,
}: RewordDialogProps) {
  const [summary, setSummary] = useState(initialSummary)
  const [body, setBody] = useState(initialBody)

  useEffect(() => {
    if (open) {
      setSummary(initialSummary)
      setBody(initialBody)
    }
  }, [open, initialSummary, initialBody])

  const trimmed = summary.trim()
  const ready = trimmed !== '' && !pending

  const submit = () => {
    if (!ready) return
    const message = body.trim() ? `${trimmed}\n\n${body.trim()}` : trimmed
    onConfirm(message)
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
            <Pencil size={15} strokeWidth={1.9} />
            Edit commit message
          </DialogTitle>
        </DialogHeader>

        <div className="grid gap-3 px-4 py-4">
          <div className="grid gap-1.5">
            <label className="text-[11px] font-semibold text-sub">Summary</label>
            <Input
              value={summary}
              onChange={(e) => setSummary(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === 'Enter') submit()
              }}
              placeholder="Short summary"
              className="h-auto bg-background py-1.5 text-xs"
              autoFocus
            />
          </div>
          <div className="grid gap-1.5">
            <label className="text-[11px] font-semibold text-sub">
              Description <span className="font-normal text-muted-foreground">(optional)</span>
            </label>
            <textarea
              value={body}
              onChange={(e) => setBody(e.target.value)}
              placeholder="More detail, if you want it"
              rows={4}
              className="w-full rounded-md border border-input bg-background px-2 py-1.5 text-xs text-foreground outline-none focus:border-ring"
            />
          </div>
          <p className="text-[10.5px] text-muted-foreground">
            Only the latest commit's message can be edited.
          </p>
        </div>

        <DialogFooter className="flex justify-end gap-2 border-t border-border px-4 py-3">
          <Button variant="secondary" size="sm" disabled={pending} onClick={() => onOpenChange(false)}>
            Cancel
          </Button>
          <Button size="sm" disabled={!ready} aria-busy={pending || undefined} onClick={submit}>
            {pending && <PendingIndicator />}
            {pending ? 'Saving…' : 'Save message'}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
