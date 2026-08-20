import { useState } from 'react'
import { toast } from 'sonner'
import { Check, Copy, Pencil } from 'lucide-react'
import type { SessionMessage } from '@/lib/bindings'
import { cn } from '@/lib/utils'
import { describeError, log } from '@/lib/log'

/** How long the Copy button shows its "Copied" confirmation before reverting. */
const COPIED_RESET_MS = 1500

/**
 * Per-message hover/focus controls for a sent user message: Copy and Edit.
 *
 * Copy is always available -- it copies `message.plainContent` and shows a
 * "Copied" confirmation on the button itself for 1.5s (Rule #1: every action
 * needs a visible response), separate from the toast pattern used elsewhere
 * so it does not spam a toast for something this quick and low-stakes.
 *
 * Edit puts the message's text back into the composer draft for that
 * session (`setDraft`) so the user can revise and resend it. This is
 * deliberately NOT a true in-place edit: `agentSessionAppendUserMessage`
 * only appends new messages, there is no backend command to replace or
 * amend a message already written to a session's transcript file, and
 * inventing one here would silently rewrite conversation history the
 * backend never agreed to change. Resend-via-composer is the honest version
 * of "edit" available today.
 *
 * There is no Cancel control here at all. `SessionState` (bindings.ts) has
 * no "queued" variant, and `SessionMessage` carries nothing describing a
 * message as pending-but-not-yet-processed -- messages are durably appended
 * one at a time by `agentSessionAppendUserMessage`, which is what makes them
 * show up in the transcript in the first place. There is nothing for a
 * Cancel button to genuinely act on, so rendering one would be a dead
 * button pretending otherwise.
 */
export function MessageActions({
  message,
  onEdit,
  className,
}: {
  message: SessionMessage
  /** Called with the message's own text; the caller owns writing it into the composer draft. */
  onEdit: (text: string) => void
  className?: string
}) {
  const [copied, setCopied] = useState(false)

  const copy = async () => {
    try {
      if (!navigator.clipboard) throw new Error('clipboard unavailable')
      await navigator.clipboard.writeText(message.plainContent)
      setCopied(true)
      window.setTimeout(() => setCopied(false), COPIED_RESET_MS)
    } catch (e) {
      const description = describeError(e)
      log.warn(`agent desk: message copy failed: ${description}`)
      toast.error('Could not copy that message.', { description })
    }
  }

  return (
    <div
      role="toolbar"
      aria-label="Message actions"
      className={cn(
        'flex items-center gap-0.5 rounded-md border border-border bg-panel2 p-0.5 opacity-0 shadow-sm transition-opacity',
        'group-hover:opacity-100 group-focus-within:opacity-100 has-[:focus-visible]:opacity-100',
        'motion-reduce:transition-none',
        className
      )}
    >
      <button
        type="button"
        onClick={() => void copy()}
        aria-label={copied ? 'Copied' : 'Copy message'}
        title={copied ? 'Copied' : 'Copy message'}
        className="flex h-6 w-6 flex-none items-center justify-center rounded text-muted-foreground hover:bg-panel3 hover:text-foreground"
      >
        {copied ? <Check size={12} aria-hidden /> : <Copy size={12} aria-hidden />}
      </button>
      <button
        type="button"
        onClick={() => onEdit(message.plainContent)}
        aria-label="Edit message"
        title="Put this message back in the composer to revise and resend"
        className="flex h-6 w-6 flex-none items-center justify-center rounded text-muted-foreground hover:bg-panel3 hover:text-foreground"
      >
        <Pencil size={12} aria-hidden />
      </button>
    </div>
  )
}
