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
import { Sentry } from '@/lib/sentry'

/** Everything a report carries besides the user's own description. */
export interface Diagnostics {
  version: string
  buildDate: string
  gitHash: string
  debug: boolean
  platform: string
  userAgent: string
  /** Scrubbed tail of gitwyrm.log, or '' when it could not be read. */
  logTail: string
  /** Set when reading the log failed, so the report says so rather than lying. */
  logError?: string
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
  includeLog: boolean
): string {
  const parts = [
    '## What happened',
    '',
    description.trim() || '_(no description given)_',
    '',
    '## Environment',
    '',
    ...environmentLines(d),
    '',
  ]

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
  d: Diagnostics
): Promise<SubmitResult> {
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
      if (d.logError) scope.setContext('log', { error: d.logError })

      return Sentry.captureFeedback(
        {
          name: 'GitWyrm user',
          email: email.trim() || undefined,
          message: scrubText(description.trim() || '(no description given)'),
        },
        {
          // The log rides as an attachment rather than inside the message: it is
          // far past Sentry's message size limit, and as an attachment it stays
          // a readable file on the issue instead of being truncated away.
          attachments: d.logTail
            ? [{ filename: 'gitwyrm.log', data: d.logTail, contentType: 'text/plain' }]
            : undefined,
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
export function githubIssueUrl(description: string, d: Diagnostics): string {
  const body = bugReportMarkdown(description, d, false)
  const trimmed =
    body.length > 6000
      ? `${body.slice(0, 6000)}\n\n_(truncated -- use "Copy report" in GitWyrm for the full details)_`
      : body
  const title = description.trim().split('\n')[0]?.slice(0, 80) || 'Bug report'
  return (
    'https://github.com/Wutname1/GitWyrm/issues/new' +
    `?title=${encodeURIComponent(title)}` +
    `&body=${encodeURIComponent(trimmed)}`
  )
}
