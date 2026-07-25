import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import App from './App'
import { describeError, log } from './lib/log'
import { hideSplash, killSplash } from './lib/splash'
import { initSentry, Sentry } from './lib/sentry'
import './index.css'

initSentry()

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
