import * as Sentry from '@sentry/react'
import { scrubDeep, scrubText } from '@/lib/scrub'

/**
 * Frontend crash reporting and observability. Mirrors the Rust backend's
 * `init_sentry`: reports only in production builds so local crashes stay local,
 * tags events with the app version as the release so they line up with the
 * backend's, and keeps PII off because repo paths and branch names travel
 * through error messages.
 *
 * Two independent opt-outs feed this, and they gate different things:
 * - `crashReports` covers errors and unhandled rejections. Off means no client
 *   is created at all, so nothing is sent.
 * - `usageTelemetry` covers performance tracing, profiling, and forwarded
 *   logs. Off drops those integrations and zeroes the sample rates, leaving
 *   crash reporting working on its own.
 *
 * The split exists so reporting the crash that would otherwise go unfixed does
 * not also mean being measured. The Rust `init_sentry` makes the same split.
 *
 * During the alpha, telemetry -- when on -- runs at full sampling, including
 * the features that would cost money on a paid plan. The free plan's quota is
 * small, so once we outgrow it the sample rates below are the dials to turn
 * down. See the `ALPHA:` comments.
 *
 * Session replay is deliberately absent. It recorded the DOM of a session that
 * ended in an error, and while every text node was masked, a git client's
 * window is the user's private repository: paths, branch names, and diffs. The
 * masking made the replays nearly unreadable anyway, so it cost privacy risk
 * for very little diagnostic value.
 *
 * Takes the user's opt-out because the client has to be configured before it
 * exists -- an initialized client opens a connection and buffers events, so
 * filtering in `beforeSend` would not honour "send nothing".
 *
 * Call once, before the app renders. Safe to call in dev -- it no-ops there.
 */
export function initSentry(enabled: boolean, usageTelemetry: boolean) {
  if (import.meta.env.DEV) return
  if (!enabled) return

  Sentry.init({
    dsn: 'https://a2cb101567f5cec264a9a0b43e6f8c24@o4511760230907904.ingest.us.sentry.io/4511769575948288',
    release: __APP_VERSION__,
    environment: 'alpha',
    sendDefaultPii: false,

    // ALPHA: forward console logs and `log.*` calls to Sentry's Logs product.
    // Usage telemetry, not crash reporting: these are the running commentary of
    // a working session, not a failure, so they follow the telemetry opt-out.
    enableLogs: usageTelemetry,

    integrations: usageTelemetry
      ? [
          // Performance tracing across fetch/navigation, plus browser vitals.
          Sentry.browserTracingIntegration(),
          // CPU profiling attached to sampled transactions.
          Sentry.browserProfilingIntegration(),
        ]
      : // Dropped rather than left at a 0.0 sample rate: browserTracing also
        // instruments fetch and history to build spans, so removing it stops
        // that work from happening at all.
        [],

    // ALPHA: trace 100% of transactions when telemetry is on. Drop toward
    // 0.1-0.2 before any real launch, or the free-plan performance quota burns
    // out fast. 0.0 leaves every transaction unsampled, so none is sent.
    tracesSampleRate: usageTelemetry ? 1.0 : 0.0,
    // ALPHA: profile 100% of the traces we sample (multiplies tracesSampleRate).
    profilesSampleRate: usageTelemetry ? 1.0 : 0.0,

    // Keep a rolling window of the user's recent actions on every event.
    maxBreadcrumbs: 100,

    // `sendDefaultPii: false` does NOT touch event payloads: an exception
    // message, a breadcrumb, or a request URL still
    // carries whatever text it was built from. Scrub both on the way out, since
    // error strings here routinely embed repo paths, author emails, and - when a
    // provider echoes a bad key back in its error body - access tokens.
    beforeSend(event) {
      return scrubDeep(event)
    },
    beforeBreadcrumb(breadcrumb) {
      return scrubDeep(breadcrumb)
    },
  })
}

/**
 * Redact a string before it is logged or shown. Re-exported so callers reach for
 * one scrubber rather than importing the module directly.
 */
export { scrubText }

export { Sentry }
