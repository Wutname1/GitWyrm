import { CircleAlert, Copy } from 'lucide-react'
import logoUrl from '@/assets/logo.png'
import { Button } from '@/components/ui/button'
import { copyToClipboard } from '@/lib/clipboard'
import { cn } from '@/lib/utils'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogTitle,
} from '@/components/ui/dialog'

export interface InProgressModalProps {
  open: boolean
  /** Headline for the running state, e.g. "Creating tag v1.0.0". */
  title: string
  /** Second line under the headline. Use it to say what is happening right now. */
  subtext?: string
  /**
   * Set once the work fails. Presence of this flips the whole modal into its
   * error state: the logo gets a red cast, the chrome comes back, and the user
   * gets Copy and Close.
   */
  error?: string | null
  /** Headline shown in place of `title` once `error` is set. */
  errorTitle?: string
  /** Called by Close, and by Esc / clicking away. Only reachable in the error state. */
  onClose: () => void
}

/**
 * Full-stop progress modal for work the user must wait on.
 *
 * While running it is deliberately bare -- no title bar, no buttons, no way
 * out -- so a half-finished git operation can't be dismissed into an unknown
 * state. It only becomes a real dialog when something goes wrong, at which
 * point the error is copyable and the modal is closable.
 */
export function InProgressModal({
  open,
  title,
  subtext,
  error,
  errorTitle = 'That did not work',
  onClose,
}: InProgressModalProps) {
  const failed = error != null

  return (
    <Dialog open={open} onOpenChange={(next) => !next && failed && onClose()}>
      <DialogContent
        className="gap-0 border-0 bg-transparent p-0 shadow-none sm:max-w-[26rem]"
        showCloseButton={false}
        // While working there is nothing to cancel into, so the usual dismissals
        // are inert. They come back with the error state.
        onEscapeKeyDown={(event) => !failed && event.preventDefault()}
        onPointerDownOutside={(event) => !failed && event.preventDefault()}
        onInteractOutside={(event) => !failed && event.preventDefault()}
        aria-describedby={subtext || error ? 'in-progress-subtext' : undefined}
      >
        <div
          className={cn(
            'wyrm-progress-panel grid justify-items-center gap-0 px-6 py-6 text-center',
            failed && 'wyrm-progress-panel-failed'
          )}
          role="status"
          aria-live="polite"
        >
          <div className="wyrm-ai-logo-wrap" aria-hidden="true">
            {!failed && <div className="wyrm-ai-energy-ring" />}
            <img
              src={logoUrl}
              alt=""
              className={failed ? 'wyrm-ai-logo wyrm-ai-logo-failed' : 'wyrm-ai-logo'}
            />
            {failed && (
              <span className="wyrm-ai-logo-fault">
                <CircleAlert size={26} strokeWidth={2.2} />
              </span>
            )}
          </div>

          <DialogTitle
            className={
              failed
                ? 'mt-3 text-sm font-semibold text-removed'
                : 'mt-3 text-sm font-semibold text-foreground'
            }
          >
            {failed ? errorTitle : title}
          </DialogTitle>

          {(subtext || failed) && (
            <DialogDescription
              id="in-progress-subtext"
              className="mt-1 max-w-full text-2xs leading-relaxed text-muted-foreground"
            >
              {failed ? subtext ?? 'Nothing was changed.' : subtext}
            </DialogDescription>
          )}

          {/* Stays mounted so the panel has a height to grow from. */}
          <div
            className={cn(
              'wyrm-progress-reveal',
              failed && 'wyrm-progress-reveal-open'
            )}
            // Collapsed, it is visually gone but still in the DOM: hide it from
            // the accessibility tree and take its buttons out of the tab order.
            aria-hidden={!failed}
            inert={!failed}
          >
            <div>
              <pre className="mt-3 max-h-40 w-full overflow-auto rounded-md border border-border bg-panel2 px-3 py-2 text-left font-mono text-2xs leading-relaxed whitespace-pre-wrap break-words text-sub">
                {error}
              </pre>
              <div className="mt-4 flex items-center justify-center gap-2">
                <Button
                  variant="secondary"
                  size="sm"
                  onClick={() => error && void copyToClipboard(error, 'Copied the error')}
                >
                  <Copy />
                  Copy error
                </Button>
                <Button size="sm" onClick={onClose}>
                  Close
                </Button>
              </div>
            </div>
          </div>
        </div>
      </DialogContent>
    </Dialog>
  )
}
