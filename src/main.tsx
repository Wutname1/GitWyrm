import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import App from './App'
import { commands } from './lib/bindings'
import { describeError, log } from './lib/log'
import { hideSplash, killSplash } from './lib/splash'
import { initSentry, Sentry } from './lib/sentry'
import './index.css'

// Crash reporting and usage telemetry are both opt-out, and honouring that
// means asking before the reporter starts. These reads are two quick commands,
// so they run ahead of the first render rather than letting the app boot
// un-gated and switching the reporter on late -- that window is exactly when
// startup crashes happen, and reporting one from a user who opted out is the
// failure this setting exists to prevent.
//
// Awaited rather than fire-and-forget so no event can be captured before the
// answer arrives. If the read fails we report crashes, matching the backend: a
// broken settings file is a first launch or corruption, not an opt-out.
//
// Usage telemetry is resolved by the backend rather than read off the settings
// field, because "unset" is not "off" -- the default depends on the version and
// update channel, and that rule lives in Rust so there is only one copy of it.
try {
  const [settings, telemetry] = await Promise.all([
    commands.getSettings(),
    commands.getUsageTelemetryEnabled(),
  ])
  initSentry(
    settings.status === 'ok' ? settings.data.crash_reports !== false : true,
    telemetry.status === 'ok' ? telemetry.data : false,
  )
} catch {
  // Crashes still report; telemetry does not, since an unknown answer should
  // not be read as consent to be measured.
  initSentry(true, false)
}

// Catch errors that escape React's boundary (event handlers, microtasks,
// unhandled rejections) so a crash leaves a visible, durable trace instead of
// silently blanking the window. Mirror them into gitwyrm.log so a bug report
// always has a durable trace, not just a console the user can't reach.
window.addEventListener('error', (e) => {
  const where = e.filename ? ` (${e.filename}:${e.lineno}:${e.colno})` : ''
  const line = `Uncaught error: ${describeError(e.error ?? e.message)}${where}`
  console.error(line)
  log.error(line)
  Sentry.captureException(e.error ?? e.message)
  // A throw before first paint would otherwise leave the splash spinning on
  // top of the error screen forever.
  killSplash()
})
window.addEventListener('unhandledrejection', (e) => {
  const line = `Unhandled rejection: ${describeError(e.reason)}`
  console.error(line)
  log.error(line)
  Sentry.captureException(e.reason)
})

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <App />
  </StrictMode>,
)

// The splash is dismissed by the launch restore in App.tsx, not here: it stays
// up until every reopened tab is ready, so startup reads as one loading state
// rather than a flash of empty app while repos stream in.
//
// Backstop: the splash covers the entire window, so if the restore ever wedges
// (an unresponsive network drive, a command that never settles) the app would
// be permanently unreachable. Lift it regardless after 15s -- a half-drawn app
// the user can act on beats a spinner they cannot dismiss.
setTimeout(hideSplash, 15_000)
