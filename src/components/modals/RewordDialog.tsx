import { useEffect, useState } from 'react'
import { Pencil } from 'lucide-react'
import { Input } from '@/components/ui/input'
import { FormDialog } from '@/components/ui/form-dialog'

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
    <FormDialog
      open={open}
      onOpenChange={onOpenChange}
      icon={<Pencil size={15} strokeWidth={1.9} />}
      title="Edit commit message"
      submitLabel="Save message"
      pendingLabel="Saving…"
      canSubmit={ready}
      pending={pending}
      onSubmit={submit}
    >
      <div className="grid gap-1.5">
        <label className="text-2xs font-semibold text-sub">Summary</label>
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
        <label className="text-2xs font-semibold text-sub">
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
    </FormDialog>
  )
}
