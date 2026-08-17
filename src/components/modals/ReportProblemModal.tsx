import { useEffect, useRef, useState } from 'react'
import { Bug, Check, Copy, ExternalLink, ImagePlus, Lightbulb, Send, X } from 'lucide-react'
import { toast } from 'sonner'
import { Button } from '@/components/ui/button'
import { Dialog, DialogContent, DialogHeader, DialogTitle } from '@/components/ui/dialog'
import { Input } from '@/components/ui/input'
import { copyToClipboard } from '@/lib/clipboard'
import { describeError } from '@/lib/log'
import {
  bugReportMarkdown,
  collectDiagnostics,
  githubIssueUrl,
  pickScreenshot,
  screenshotFromTransfer,
  submitFeedback,
  type Diagnostics,
  type ReportKind,
  type Screenshot,
} from '@/lib/feedback'
import { cn } from '@/lib/utils'

/** Wording that differs between the two modes, kept together to stay consistent. */
const COPY: Record<
  ReportKind,
  { title: string; prompt: string; placeholder: string; send: string; needsText: string }
> = {
  bug: {
    title: 'Report a problem',
    prompt: 'What went wrong?',
    placeholder: 'What were you doing, and what happened instead of what you expected?',
    send: 'Send report',
    needsText: 'Tell us what went wrong first',
  },
  feedback: {
    title: 'Send feedback',
    prompt: "What's on your mind?",
    placeholder: 'An idea, something that felt awkward, or anything you wish GitWyrm did.',
    send: 'Send feedback',
    needsText: 'Write your feedback first',
  },
}

/**
 * Bug reports and feedback, in one dialog.
 *
 * Both are "the user wants to tell us something", so they share a form rather
 * than splitting into two nearly identical dialogs. What differs is what gets
 * attached: a bug is unactionable without the log, while an idea has no reason
 * to carry one -- so the log rides along by default for a bug and is off by
 * default for feedback, and either way the checkbox is right there.
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
  initialKind = 'bug',
}: {
  open: boolean
  onClose: () => void
  initialDescription?: string
  initialKind?: ReportKind
}) {
  const [kind, setKind] = useState<ReportKind>(initialKind)
  const [description, setDescription] = useState(initialDescription)
  const [email, setEmail] = useState('')
  const [diagnostics, setDiagnostics] = useState<Diagnostics | null>(null)
  const [includeLog, setIncludeLog] = useState(initialKind === 'bug')
  const [screenshot, setScreenshot] = useState<Screenshot | null>(null)
  const [sending, setSending] = useState(false)
  const [sent, setSent] = useState(false)
  const [showLog, setShowLog] = useState(false)
  // Set once the user touches the checkbox, after which switching mode must not
  // move it back -- an explicit choice outranks the default for the new mode.
  const logChoiceMade = useRef(false)

  const text = COPY[kind]

  useEffect(() => {
    if (!open) return
    setSent(false)
    setShowLog(false)
    setKind(initialKind)
    setDescription(initialDescription)
    setDiagnostics(null)
    setScreenshot(null)
    setIncludeLog(initialKind === 'bug')
    logChoiceMade.current = false
    void collectDiagnostics().then(setDiagnostics)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open])

  const switchKind = (next: ReportKind) => {
    setKind(next)
    if (!logChoiceMade.current) setIncludeLog(next === 'bug')
  }

  const attachScreenshot = async () => {
    try {
      const picked = await pickScreenshot()
      if (picked) setScreenshot(picked)
    } catch (e) {
      toast.error(`Could not attach that image: ${describeError(e)}`)
    }
  }

  /** Accept a pasted or dropped image, so Win+Shift+S goes straight in. */
  const takeTransfer = async (data: DataTransfer | null) => {
    try {
      const shot = await screenshotFromTransfer(data)
      if (!shot) return false
      setScreenshot(shot)
      toast.success('Screenshot attached')
      return true
    } catch (e) {
      toast.error(`Could not attach that image: ${describeError(e)}`)
      return true
    }
  }

  const send = async () => {
    if (!diagnostics) return
    setSending(true)
    const result = await submitFeedback(description, email, diagnostics, {
      kind,
      includeLog,
      screenshot,
    })
    setSending(false)

    if (result.ok) {
      setSent(true)
      toast.success(kind === 'bug' ? 'Report sent. Thank you!' : 'Feedback sent. Thank you!')
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
    toast.error(`Could not send it: ${result.message}`, {
      description: 'Use "Copy report" and open a GitHub issue instead.',
    })
  }

  const copyReport = () => {
    if (!diagnostics) return
    void copyToClipboard(
      bugReportMarkdown(description, diagnostics, includeLog, kind),
      includeLog ? 'Copied, log included' : 'Copied'
    )
  }

  const openGithub = () => {
    if (!diagnostics) return
    const url = githubIssueUrl(description, diagnostics, kind)
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
            {kind === 'bug' ? (
              <Bug size={14} className="text-accent-text" />
            ) : (
              <Lightbulb size={14} className="text-accent-text" />
            )}
            {text.title}
          </DialogTitle>
        </DialogHeader>

        <div className="min-h-0 flex-1 space-y-4 overflow-y-auto px-4 py-4">
          {sent ? (
            <div className="flex flex-col items-center gap-2 py-8 text-center">
              <div className="grid size-10 place-items-center rounded-full bg-accent/15">
                <Check size={20} className="text-accent-text" />
              </div>
              <div className="text-sm font-semibold text-foreground">
                {kind === 'bug' ? 'Report sent' : 'Feedback sent'}
              </div>
              <p className="max-w-sm text-2xs leading-relaxed text-muted-foreground">
                Thank you. {includeLog ? 'Your log was included, so there is nothing else you need to do. ' : ''}
                If you gave an email address we may follow up.
              </p>
            </div>
          ) : (
            <>
              <div
                role="radiogroup"
                aria-label="What kind of message is this?"
                className="grid grid-cols-2 gap-2"
              >
                {(['bug', 'feedback'] as const).map((option) => {
                  const active = kind === option
                  const Icon = option === 'bug' ? Bug : Lightbulb
                  return (
                    <button
                      key={option}
                      type="button"
                      role="radio"
                      aria-checked={active}
                      onClick={() => switchKind(option)}
                      className={cn(
                        'flex items-start gap-2 rounded-md border p-2.5 text-left transition-colors',
                        active
                          ? 'border-accent bg-accent/10'
                          : 'border-border bg-panel2 hover:border-muted-foreground/40'
                      )}
                    >
                      <Icon
                        size={14}
                        className={cn(
                          'mt-0.5 flex-none',
                          active ? 'text-accent-text' : 'text-muted-foreground'
                        )}
                      />
                      <span className="min-w-0">
                        <span className="block text-xs font-medium text-foreground">
                          {option === 'bug' ? "Something's broken" : 'Share an idea'}
                        </span>
                        <span className="mt-0.5 block text-2xs leading-relaxed text-muted-foreground">
                          {option === 'bug'
                            ? 'A crash, an error, or something that did the wrong thing'
                            : 'A suggestion, or something that could work better'}
                        </span>
                      </span>
                    </button>
                  )
                })}
              </div>

              <div>
                <label
                  htmlFor="report-description"
                  className="mb-1.5 block text-xs font-medium text-foreground"
                >
                  {text.prompt}
                </label>
                <textarea
                  id="report-description"
                  value={description}
                  onChange={(e) => setDescription(e.target.value)}
                  onPaste={(e) => {
                    // Only swallow the paste when it actually carried an image,
                    // so pasting text still lands in the box as normal.
                    void screenshotFromTransfer(e.clipboardData).then((shot) => {
                      if (shot) setScreenshot(shot)
                    })
                    if (Array.from(e.clipboardData.files).some((f) => f.type.startsWith('image/'))) {
                      e.preventDefault()
                      toast.success('Screenshot attached')
                    }
                  }}
                  onDrop={(e) => {
                    if (Array.from(e.dataTransfer.files).some((f) => f.type.startsWith('image/'))) {
                      e.preventDefault()
                      void takeTransfer(e.dataTransfer)
                    }
                  }}
                  rows={5}
                  autoFocus
                  placeholder={text.placeholder}
                  className="w-full resize-y rounded-md border border-input bg-background p-2.5 text-xs leading-relaxed text-foreground outline-none placeholder:text-muted-foreground focus:border-ring"
                />
                <p className="mt-1 text-2xs text-muted-foreground">
                  Tip: press Windows + Shift + S to grab a screenshot, then paste it here.
                </p>
              </div>

              <div className="rounded-md border border-border bg-panel2 p-3">
                {screenshot ? (
                  <div className="flex items-start gap-3">
                    <img
                      src={screenshot.dataUrl}
                      alt="The screenshot you attached"
                      className="size-16 flex-none rounded border border-border object-cover"
                    />
                    <div className="min-w-0 flex-1">
                      <div className="text-xs font-medium text-foreground">Screenshot attached</div>
                      <p className="mt-0.5 truncate text-2xs text-muted-foreground">
                        {screenshot.name}
                      </p>
                      <button
                        onClick={() => void attachScreenshot()}
                        className="mt-1 text-2xs font-medium text-accent-text hover:underline"
                      >
                        Choose a different one
                      </button>
                    </div>
                    <button
                      onClick={() => setScreenshot(null)}
                      aria-label="Remove the screenshot"
                      className="flex-none rounded p-1 text-muted-foreground hover:bg-background hover:text-foreground"
                    >
                      <X size={14} />
                    </button>
                  </div>
                ) : (
                  <div className="flex items-start justify-between gap-3">
                    <div className="min-w-0">
                      <div className="text-xs font-medium text-foreground">
                        Add a screenshot <span className="text-muted-foreground">(optional)</span>
                      </div>
                      <p className="mt-0.5 text-2xs leading-relaxed text-muted-foreground">
                        A picture of the problem often explains it faster than words can.
                      </p>
                    </div>
                    <Button
                      variant="secondary"
                      size="sm"
                      className="h-7 flex-none gap-1.5 text-2xs"
                      onClick={() => void attachScreenshot()}
                    >
                      <ImagePlus size={12} />
                      Choose image
                    </Button>
                  </div>
                )}
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
                    <label className="flex cursor-pointer items-center gap-2">
                      <input
                        type="checkbox"
                        checked={includeLog}
                        onChange={(e) => {
                          logChoiceMade.current = true
                          setIncludeLog(e.target.checked)
                        }}
                        className="size-3.5 accent-accent"
                      />
                      <span className="text-xs font-medium text-foreground">
                        Include my log{timingLines > 0 ? ' and recent timings' : ''}
                      </span>
                    </label>
                    <p className="mt-0.5 text-2xs leading-relaxed text-muted-foreground">
                      {!includeLog ? (
                        kind === 'bug' ? (
                          <span className="text-amber-300">
                            Without the log a problem is much harder to track down.
                          </span>
                        ) : (
                          'Only your message and email will be sent.'
                        )
                      ) : !diagnostics ? (
                        'Collecting…'
                      ) : diagnostics.logError ? (
                        <span className="text-amber-300">
                          The log could not be read, so it will go without it.
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
                        'The log is empty, so it will go without it.'
                      )}
                    </p>
                  </div>
                  {includeLog && (diagnostics?.logTail || diagnostics?.perfTrail) && (
                    <button
                      onClick={() => setShowLog((v) => !v)}
                      className="flex-none text-2xs font-medium text-accent-text hover:underline"
                    >
                      {showLog ? 'Hide' : 'Show me'}
                    </button>
                  )}
                </div>
                {includeLog && showLog && diagnostics?.perfTrail && (
                  <pre className="mt-2.5 max-h-32 overflow-auto rounded border border-border bg-background p-2 font-mono text-[10px] leading-[1.5] text-sub">
                    {diagnostics.perfTrail}
                  </pre>
                )}
                {includeLog && showLog && diagnostics?.logTail && (
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
                tooltip={!description.trim() ? text.needsText : undefined}
              >
                <Send size={12} />
                {sending ? 'Sending…' : text.send}
              </Button>
            </>
          )}
        </div>
      </DialogContent>
    </Dialog>
  )
}
