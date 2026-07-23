import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import App from './App'
import { describeError, log } from './lib/log'
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
