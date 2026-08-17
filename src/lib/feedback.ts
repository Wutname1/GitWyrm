/**
 * One-click bug reporting.
 *
 * A report is only worth as much as the context attached to it, and the context
 * that matters most -- the application log -- is exactly the part a user will
 * not paste by hand. So `submitFeedback` gathers it for them: the log tail, the
 * build info, and the platform, scrubbed through the same redactor that guards
 * crash reports, then sent to Sentry as a feedback event with the log as an
 * attachment.
 *
 * Sentry is the one-click path. `bugReportMarkdown` builds the same content for
 * the GitHub route, where the user pastes it into a public issue instead.
 */

import { commands } from '@/lib/bindings'
import { unwrap } from '@/lib/queryKeys'
import { scrubText } from '@/lib/scrub'
import { log, describeError } from '@/lib/log'
import { perfMarksText } from '@/lib/perfTrail'
import { Sentry } from '@/lib/sentry'

/**
 * What the user is here to say.
 *
 * The two differ in what they need attached, not just in wording: a bug is
 * unactionable without the log, while an idea is a message that happens to come
 * from inside the app and carries no reason to ship a log with it.
 */
export type ReportKind = 'bug' | 'feedback'

/** A screenshot the user chose to attach, held as a data URL. */
export interface Screenshot {
  /** `data:image/png;base64,...` -- previewed in an `<img>` and sent as-is. */
  dataUrl: string
  /** Shown next to the thumbnail so the user knows which image this is. */
  name: string
}

/** Everything a report carries besides the user's own description. */
export interface Diagnostics {
  version: string
  buildDate: string
  gitHash: string
  debug: boolean
  platform: string
  userAgent: string
  /** Scrubbed tail of the app log, or '' when it could not be read. */
  logTail: string
  /** Set when reading the log failed, so the report says so rather than lying. */
  logError?: string
  /**
   * Recent timings, newest last. Empty when nothing has been measured yet.
   *
   * Carried because "it is slow" is the one complaint a log tail answers
   * badly: the durations live in Sentry's performance dataset, keyed by
   * nothing that connects them to the report. Attaching them here means a
   * report about a slow open arrives with the actual number on it.
   */
  perfTrail: string
}

/**
 * Collect diagnostics for a report. Never throws: a report with a missing
 * section is far better than a reporting button that fails.
 */
export async function collectDiagnostics(): Promise<Diagnostics> {
  const base: Diagnostics = {
    version: __APP_VERSION__,
    buildDate: 'unknown',
    gitHash: 'unknown',
    debug: false,
    platform: navigator.platform,
    userAgent: navigator.userAgent,
    logTail: '',
    // Read straight away: this is in-memory and cannot fail, unlike the log.
    perfTrail: perfMarksText(),
  }

  try {
    const build = await commands.buildInfo()
    base.version = build.version
    base.buildDate = build.build_date
    base.gitHash = build.git_hash
    base.debug = build.debug
  } catch (e) {
    log.warn(`bug report: could not read build info: ${describeError(e)}`)
  }

  try {
    base.logTail = scrubText(unwrap(await commands.readLogTail()))
  } catch (e) {
    base.logError = describeError(e)
    log.warn(`bug report: could not read log tail: ${base.logError}`)
  }

  return base
}

/** Image types both the picker and the paste path accept. */
const SCREENSHOT_EXTENSIONS = ['png', 'jpg', 'jpeg', 'webp', 'gif']

/**
 * Ask for a screenshot with the system file picker.
 *
 * Resolves to null when the user cancels, which is not an error and should not
 * be reported as one. Throws only when a chosen file could not be read.
 */
export async function pickScreenshot(): Promise<Screenshot | null> {
  const { open } = await import('@tauri-apps/plugin-dialog')
  const selected = await open({
    title: 'Choose a screenshot',
    multiple: false,
    directory: false,
    filters: [{ name: 'Images', extensions: SCREENSHOT_EXTENSIONS }],
  })
  if (typeof selected !== 'string') return null

  // The webview has no filesystem access, so the bytes come back through Rust.
  const dataUrl = unwrap(await commands.readScreenshot(selected))
  const name = selected.split(/[\\/]/).pop() || 'screenshot'
  return { dataUrl, name }
}

/**
 * Pull an image out of a paste or drop, if there is one.
 *
 * This is the path that matters most on Windows: Win+Shift+S puts the capture
 * straight on the clipboard, so requiring a saved file first would add a step
 * to the common case for no reason.
 */
export async function screenshotFromTransfer(
  data: DataTransfer | null
): Promise<Screenshot | null> {
  const file = Array.from(data?.files ?? []).find((f) => f.type.startsWith('image/'))
  if (!file) return null
  if (file.size > MAX_SCREENSHOT_BYTES) {
    throw new Error('That image is too large. Choose one under 8 MB.')
  }

  const dataUrl = await new Promise<string>((resolve, reject) => {
    const reader = new FileReader()
    reader.onload = () => resolve(String(reader.result))
    reader.onerror = () => reject(new Error('That image could not be read.'))
    reader.readAsDataURL(file)
  })
  return { dataUrl, name: file.name || 'pasted image' }
}

/** Mirrors the Rust-side cap so a paste is rejected before it is decoded. */
const MAX_SCREENSHOT_BYTES = 8 * 1024 * 1024

/** The environment block, shared by the Sentry context and the markdown body. */
function environmentLines(d: Diagnostics): string[] {
  return [
    `- GitWyrm: ${d.version}${d.debug ? ' (debug)' : ''}`,
    `- Build: ${d.buildDate} · ${d.gitHash}`,
    `- Platform: ${d.platform}`,
    `- User agent: ${d.userAgent}`,
  ]
}

/**
 * Format a report as markdown, ready to paste into a GitHub issue.
 *
 * `includeLog` is false for the URL-prefill path: GitHub rejects issue URLs
 * past roughly 8k characters, and a 200KB log would blow straight through it.
 * The copy-to-clipboard path passes true and carries the whole thing.
 */
export function bugReportMarkdown(
  description: string,
  d: Diagnostics,
  includeLog: boolean,
  kind: ReportKind = 'bug'
): string {
  const parts = [
    kind === 'bug' ? '## What happened' : '## Feedback',
    '',
    description.trim() || '_(no description given)_',
    '',
    '## Environment',
    '',
    ...environmentLines(d),
    '',
  ]

  // A feedback note is a message, not a diagnosis: the log and timings below
  // describe the machine and have nothing to say about an idea.
  if (kind === 'feedback') return parts.join('\n')

  // Ahead of the log: it is a handful of lines and it is the part that answers
  // "why was it slow", which the log tail buries.
  if (d.perfTrail) {
    parts.push('## Recent timings', '', '```', d.perfTrail, '```', '')
  }

  if (d.logError) {
    parts.push('## Log', '', `_Could not read the log: ${d.logError}_`, '')
  } else if (includeLog && d.logTail) {
    parts.push('## Log', '', '```', d.logTail, '```', '')
  } else if (d.logTail) {
    parts.push(
      '## Log',
      '',
      '_Not included here -- use "Copy report" in GitWyrm to get the full log._',
      ''
    )
  }

  return parts.join('\n')
}

/** Split a data URL into its mime type and decoded bytes. */
function decodeDataUrl(dataUrl: string): { bytes: Uint8Array; contentType: string } | null {
  const match = /^data:([^;,]+);base64,(.*)$/s.exec(dataUrl)
  if (!match) return null
  try {
    const binary = atob(match[2])
    const bytes = new Uint8Array(binary.length)
    for (let i = 0; i < binary.length; i += 1) bytes[i] = binary.charCodeAt(i)
    return { bytes, contentType: match[1] }
  } catch {
    return null
  }
}

type Attachment = {
  filename: string
  data: string | Uint8Array
  contentType: string
}

/**
 * Files to attach to a report: the log tail, the recent timings, and any
 * screenshot the user picked.
 *
 * They ride as attachments rather than inside the message -- the log is far
 * past Sentry's message size limit, and as files they stay readable on the
 * issue instead of being truncated away. Returns undefined rather than an
 * empty array when there is nothing to attach.
 *
 * Nothing here passes through Sentry's `beforeSend`, which only sees the event
 * envelope: every string attached below must already be scrubbed by the time it
 * reaches this function. `logTail` is scrubbed in `collectDiagnostics`, and the
 * timings are built from labels and numbers that never carry user text.
 */
export function attachmentsFor(
  d: Diagnostics,
  opts: { includeLog: boolean; screenshot?: Screenshot | null } = { includeLog: true }
): Attachment[] | undefined {
  const files: Attachment[] = []
  // The log and the timings are one decision, not two: both describe the
  // machine rather than the message, so opting out has to drop both or the
  // opt-out is a half-truth.
  if (opts.includeLog && d.logTail) {
    files.push({ filename: 'gitwyrm.log', data: d.logTail, contentType: 'text/plain' })
  }
  if (opts.includeLog && d.perfTrail) {
    files.push({ filename: 'timings.txt', data: d.perfTrail, contentType: 'text/plain' })
  }
  if (opts.screenshot) {
    const decoded = decodeDataUrl(opts.screenshot.dataUrl)
    // Sent as bytes rather than the data URL string: Sentry stores an
    // attachment verbatim, so a base64 payload would download as unviewable
    // text instead of an image.
    if (decoded) {
      files.push({
        filename: screenshotFilename(opts.screenshot.name, decoded.contentType),
        data: decoded.bytes,
        contentType: decoded.contentType,
      })
    }
  }
  return files.length > 0 ? files : undefined
}

/**
 * A safe filename for the attached screenshot.
 *
 * The user's own filename is discarded rather than sanitised -- it is chosen
 * from their disk and can name a project, a client, or a person, none of which
 * the report needs.
 */
function screenshotFilename(_name: string, contentType: string): string {
  const ext = contentType.split('/')[1]?.replace('jpeg', 'jpg') ?? 'png'
  return `screenshot.${ext}`
}

export type SubmitResult =
  | { ok: true; eventId: string }
  | { ok: false; reason: 'disabled' | 'failed'; message: string }

/**
 * Send a report to Sentry with the log attached.
 *
 * Returns a typed outcome rather than throwing, so the caller can tell the user
 * exactly what happened -- and specifically can distinguish "Sentry is off in
 * this build" from "the upload failed", which look identical otherwise.
 */
export async function submitFeedback(
  description: string,
  email: string,
  d: Diagnostics,
  opts: {
    kind?: ReportKind
    includeLog?: boolean
    screenshot?: Screenshot | null
  } = {}
): Promise<SubmitResult> {
  const kind = opts.kind ?? 'bug'
  const includeLog = opts.includeLog ?? kind === 'bug'
  // No client means nothing can be sent: either this is a dev build, or the
  // user turned crash reports off. Say which, rather than reporting a success
  // that never left the machine -- someone who opted out and later files a bug
  // needs to know the switch is why, and where to undo it.
  if (!Sentry.getClient()) {
    return {
      ok: false,
      reason: 'disabled',
      message: import.meta.env.DEV
        ? 'Reporting is turned off in development builds.'
        : 'Crash reports are turned off, so this could not be sent. You can turn them back on in Settings > Behavior.',
    }
  }

  try {
    // Capture inside withScope so the context and tag below apply to this event
    // and nothing after it. Scope changes do not survive the callback, so the
    // capture has to happen within it rather than after.
    const eventId = Sentry.withScope((scope) => {
      scope.setContext('gitwyrm', {
        version: d.version,
        build_date: d.buildDate,
        git_hash: d.gitHash,
        debug: d.debug,
        platform: d.platform,
      })
      scope.setTag('report', 'user-feedback')
      // Ideas and bug reports land in the same Sentry project but want
      // different queues, so the kind has to be filterable rather than only
      // readable in the message body.
      scope.setTag('report_kind', kind)
      scope.setTag('has_screenshot', opts.screenshot ? 'yes' : 'no')
      if (includeLog && d.logError) scope.setContext('log', { error: d.logError })
      // On the event itself, not only the attachment: a context block is
      // visible on the issue page without downloading anything, which is what
      // makes a "this is slow" report triageable at a glance.
      if (includeLog && d.perfTrail) {
        scope.setContext('recent_timings', { trail: d.perfTrail })
      }

      return Sentry.captureFeedback(
        {
          name: 'GitWyrm user',
          email: email.trim() || undefined,
          message: scrubText(description.trim() || '(no description given)'),
        },
        {
          attachments: attachmentsFor(d, { includeLog, screenshot: opts.screenshot }),
        },
        scope
      )
    })

    if (!eventId) {
      return { ok: false, reason: 'failed', message: 'Sentry did not accept the report.' }
    }

    // Force the event out now rather than on the next natural flush: the user
    // may close the app immediately after reporting.
    await Sentry.flush(5000)

    log.info(`bug report submitted: event=${eventId}`)
    return { ok: true, eventId }
  } catch (e) {
    const message = describeError(e)
    log.warn(`bug report failed to send: ${message}`)
    return { ok: false, reason: 'failed', message }
  }
}

/** Prefilled GitHub issue URL, trimmed to what GitHub will accept. */
export function githubIssueUrl(
  description: string,
  d: Diagnostics,
  kind: ReportKind = 'bug'
): string {
  const body = bugReportMarkdown(description, d, false, kind)
  const trimmed =
    body.length > 6000
      ? `${body.slice(0, 6000)}\n\n_(truncated -- use "Copy report" in GitWyrm for the full details)_`
      : body
  const title =
    description.trim().split('\n')[0]?.slice(0, 80) ||
    (kind === 'bug' ? 'Bug report' : 'Feedback')
  return (
    'https://github.com/Wutname1/GitWyrm/issues/new' +
    `?title=${encodeURIComponent(title)}` +
    `&body=${encodeURIComponent(trimmed)}`
  )
}
