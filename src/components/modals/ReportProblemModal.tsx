import { useEffect, useState } from 'react'
import { Bug, Check, Copy, ExternalLink, Send } from 'lucide-react'
import { toast } from 'sonner'
import { Button } from '@/components/ui/button'
import { Dialog, DialogContent, DialogHeader, DialogTitle } from '@/components/ui/dialog'
import { Input } from '@/components/ui/input'
import { copyToClipboard } from '@/lib/clipboard'
import {
  bugReportMarkdown,
  collectDiagnostics,
  githubIssueUrl,
  submitFeedback,
  type Diagnostics,
} from '@/lib/feedback'
import { cn } from '@/lib/utils'

/**
 * One-click bug reporting.
 *
 * The log is gathered as soon as the dialog opens rather than on submit, so the
 * user can see exactly what will be sent before deciding to send it -- and so
 * the log reflects the moment the problem happened, not the moment they finished
 * typing.
 */
export function ReportProblemModal({
  open,
  onClose,
  initialDescription = '',
}: {
  open: boolean
  onClose: () => void
  initialDescription?: string
}) {
  const [description, setDescription] = useState(initialDescription)
  const [email, setEmail] = useState('')
  const [diagnostics, setDiagnostics] = useState<Diagnostics | null>(null)
  const [sending, setSending] = useState(false)
  const [sent, setSent] = useState(false)
  const [showLog, setShowLog] = useState(false)

  useEffect(() => {
    if (!open) return
    setSent(false)
    setShowLog(false)
    setDescription(initialDescription)
    setDiagnostics(null)
    void collectDiagnostics().then(setDiagnostics)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open])

  const send = async () => {
    if (!diagnostics) return
    setSending(true)
    const result = await submitFeedback(description, email, diagnostics)
    setSending(false)

    if (result.ok) {
      setSent(true)
      toast.success('Report sent. Thank you!')
      return
    }
    // Sentry being off is not a failure the user caused, so point them at the
    // route that still works instead of just saying "failed".
    if (result.reason === 'disabled') {
      toast.error(result.message, {
        description: 'Use "Copy report" and open a GitHub issue instead.',
      })
      return
    }
    toast.error(`Could not send the report: ${result.message}`, {
      description: 'Use "Copy report" and open a GitHub issue instead.',
    })
  }

  const copyReport = () => {
    if (!diagnostics) return
    void copyToClipboard(
      bugReportMarkdown(description, diagnostics, true),
      'Report copied, log included'
    )
  }

  const openGithub = () => {
    if (!diagnostics) return
    const url = githubIssueUrl(description, diagnostics)
    void import('@tauri-apps/plugin-opener').then(({ openUrl }) => openUrl(url))
  }

  const logLines = diagnostics?.logTail ? diagnostics.logTail.split('\n').length : 0
  // Recent durations ride along with the log. Counted so the panel can say so:
  // the point of this box is that nothing is sent the user was not shown.
  const timingLines = diagnostics?.perfTrail ? diagnostics.perfTrail.split('\n').length : 0

  return (
    <Dialog open={open} onOpenChange={(o) => !o && onClose()}>
      <DialogContent className="flex max-h-[80vh] flex-col gap-0 p-0 sm:max-w-2xl" aria-describedby={undefined}>
        <DialogHeader className="flex-none border-b border-border px-4 pb-3 pt-4">
          <DialogTitle className="flex items-center gap-2 text-sm">
            <Bug size={14} className="text-accent-text" />
            Report a problem
          </DialogTitle>
        </DialogHeader>

        <div className="min-h-0 flex-1 space-y-4 overflow-y-auto px-4 py-4">
          {sent ? (
            <div className="flex flex-col items-center gap-2 py-8 text-center">
              <div className="grid size-10 place-items-center rounded-full bg-accent/15">
                <Check size={20} className="text-accent-text" />
              </div>
              <div className="text-sm font-semibold text-foreground">Report sent</div>
              <p className="max-w-sm text-2xs leading-relaxed text-muted-foreground">
                Thank you. Your log was included, so there is nothing else you need
                to do. If you gave an email address we may follow up.
              </p>
            </div>
          ) : (
            <>
              <div>
                <label
                  htmlFor="report-description"
                  className="mb-1.5 block text-xs font-medium text-foreground"
                >
                  What went wrong?
                </label>
                <textarea
                  id="report-description"
                  value={description}
                  onChange={(e) => setDescription(e.target.value)}
                  rows={5}
                  autoFocus
                  placeholder="What were you doing, and what happened instead of what you expected?"
                  className="w-full resize-y rounded-md border border-input bg-background p-2.5 text-xs leading-relaxed text-foreground outline-none placeholder:text-muted-foreground focus:border-ring"
                />
              </div>

              <div>
                <label
                  htmlFor="report-email"
                  className="mb-1.5 block text-xs font-medium text-foreground"
                >
                  Your email <span className="text-muted-foreground">(optional)</span>
                </label>
                <Input
                  id="report-email"
                  type="email"
                  value={email}
                  onChange={(e) => setEmail(e.target.value)}
                  placeholder="So we can ask a follow-up question"
                  className="h-8 bg-background text-xs"
                />
              </div>

              <div className="rounded-md border border-border bg-panel2 p-3">
                <div className="flex items-start justify-between gap-3">
                  <div className="min-w-0">
                    <div className="text-xs font-medium text-foreground">
                      Your log{timingLines > 0 ? ' and recent timings are' : ' is'} attached
                      automatically
                    </div>
                    <p className="mt-0.5 text-2xs leading-relaxed text-muted-foreground">
                      {!diagnostics ? (
                        'Collecting…'
                      ) : diagnostics.logError ? (
                        <span className="text-amber-300">
                          The log could not be read, so the report will go without it.
                        </span>
                      ) : logLines > 0 ? (
                        <>
                          The last {logLines.toLocaleString()} lines, with tokens,
                          emails, and your username removed
                          {timingLines > 0 && (
                            <>
                              , plus how long the last {timingLines.toLocaleString()}{' '}
                              {timingLines === 1 ? 'action' : 'actions'} took
                            </>
                          )}
                          . GitWyrm {diagnostics.version} · {diagnostics.platform}
                        </>
                      ) : (
                        'The log is empty, so the report will go without it.'
                      )}
                    </p>
                  </div>
                  {(diagnostics?.logTail || diagnostics?.perfTrail) && (
                    <button
                      onClick={() => setShowLog((v) => !v)}
                      className="flex-none text-2xs font-medium text-accent-text hover:underline"
                    >
                      {showLog ? 'Hide' : 'Show me'}
                    </button>
                  )}
                </div>
                {showLog && diagnostics?.perfTrail && (
                  <pre className="mt-2.5 max-h-32 overflow-auto rounded border border-border bg-background p-2 font-mono text-[10px] leading-[1.5] text-sub">
                    {diagnostics.perfTrail}
                  </pre>
                )}
                {showLog && diagnostics?.logTail && (
                  <pre className="mt-2.5 max-h-56 overflow-auto rounded border border-border bg-background p-2 font-mono text-[10px] leading-[1.5] text-sub">
                    {diagnostics.logTail}
                  </pre>
                )}
              </div>
            </>
          )}
        </div>

        <div className="flex flex-none items-center gap-2 border-t border-border px-4 py-3">
          {sent ? (
            <>
              <div className="flex-1" />
              <Button size="sm" className="h-8 text-xs" onClick={onClose}>
                Done
              </Button>
            </>
          ) : (
            <>
              <Button
                variant="secondary"
                size="sm"
                className="h-8 gap-1.5 text-xs"
                disabled={!diagnostics}
                onClick={copyReport}
              >
                <Copy size={12} />
                Copy report
              </Button>
              <Button
                variant="secondary"
                size="sm"
                className="h-8 gap-1.5 text-xs"
                disabled={!diagnostics}
                onClick={openGithub}
              >
                <ExternalLink size={12} />
                Open a GitHub issue
              </Button>
              <div className="flex-1" />
              <Button
                size="sm"
                className={cn('h-8 gap-1.5 text-xs')}
                disabled={!diagnostics || sending || !description.trim()}
                onClick={send}
                tooltip={!description.trim() ? 'Tell us what went wrong first' : undefined}
              >
                <Send size={12} />
                {sending ? 'Sending…' : 'Send report'}
              </Button>
            </>
          )}
        </div>
      </DialogContent>
    </Dialog>
  )
}
