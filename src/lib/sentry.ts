import * as Sentry from '@sentry/react'

/**
 * Frontend crash reporting and observability. Mirrors the Rust backend's
 * `init_sentry`: reports only in production builds so local crashes stay local,
 * tags events with the app version as the release so they line up with the
 * backend's, and keeps PII off because repo paths and branch names travel
 * through error messages.
 *
 * During the alpha we turn on everything the Sentry SDK offers -- performance
 * tracing, session replay, and profiling -- even the features that would cost
 * money on a paid plan. The free plan's quota is small, so once we outgrow it
 * (or move to the self-hosted server) the sample rates below are the dials to
 * turn down. See the `ALPHA:` comments.
 *
 * Call once, before the app renders. Safe to call in dev -- it no-ops there.
 */
export function initSentry() {
  if (import.meta.env.DEV) return

  Sentry.init({
    dsn: 'https://a2cb101567f5cec264a9a0b43e6f8c24@o4511760444686336.ingest.us.sentry.io/4511769575948288',
    release: __APP_VERSION__,
    environment: 'alpha',
    sendDefaultPii: false,

    // ALPHA: forward console logs and `log.*` calls to Sentry's Logs product.
    enableLogs: true,

    integrations: [
      // Performance tracing across fetch/navigation, plus browser vitals.
      Sentry.browserTracingIntegration(),
      // CPU profiling attached to sampled transactions.
      Sentry.browserProfilingIntegration(),
      // Records a video-like DOM replay of the sessions that end in an error.
      Sentry.replayIntegration({
        // Repo paths, branch names, and diffs must never leave the machine, so
        // mask every text node and block every input by default.
        maskAllText: true,
        blockAllMedia: true,
        maskAllInputs: true,
      }),
    ],

    // ALPHA: trace 100% of transactions. Drop toward 0.1-0.2 before any real
    // launch, or the free-plan performance quota burns out fast.
    tracesSampleRate: 1.0,
    // ALPHA: profile 100% of the traces we sample (multiplies tracesSampleRate).
    profilesSampleRate: 1.0,

    // ALPHA: never replay a healthy session (0.0), but always capture the
    // session when an error fires (1.0) -- the errors are what we want to watch.
    replaysSessionSampleRate: 0.0,
    replaysOnErrorSampleRate: 1.0,

    // Keep a rolling window of the user's recent actions on every event.
    maxBreadcrumbs: 100,
  })
}

export { Sentry }
