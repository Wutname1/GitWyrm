import { Component, type ErrorInfo, type ReactNode } from 'react'
import { log, describeError } from '../lib/log'
import { Sentry } from '../lib/sentry'
import { CrashTitleBar } from './CrashTitleBar'
import { ReportProblemModal } from './modals/ReportProblemModal'

interface Props {
  children: ReactNode
}

interface State {
  error: Error | null
  info: ErrorInfo | null
  copied: boolean
  reporting: boolean
}

/**
 * Catches render-time exceptions so an unexpected throw shows a readable error
 * instead of tearing the whole React tree down to a blank window. Without this,
 * an external repo change that produces unexpected data could crash the UI with
 * no visible cause.
 */
export class ErrorBoundary extends Component<Props, State> {
  state: State = { error: null, info: null, copied: false, reporting: false }

  static getDerivedStateFromError(error: Error): Partial<State> {
    return { error }
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    this.setState({ info })
    // Surface to the backend log too, so a crash leaves a durable trace.
    console.error('UI crashed:', error, info.componentStack)
    log.error(`UI crashed: ${describeError(error)}\n${info.componentStack ?? ''}`)
    Sentry.captureException(error, {
      contexts: { react: { componentStack: info.componentStack } },
    })
  }

  /** Format the crash as a markdown bug report ready to paste into an issue. */
  private bugReport(): string {
    const { error, info } = this.state
    const stack = error?.stack?.trim()
    const componentStack = info?.componentStack?.trim()
    return [
      '## Bug report',
      '',
      '**What happened:** <!-- what were you doing when this appeared? -->',
      '',
      '**Error:**',
      '',
      '```',
      error ? describeError(error) : '(unknown)',
      '```',
      '',
      ...(stack && stack !== describeError(error)
        ? ['**Stack trace:**', '', '```', stack, '```', '']
        : []),
      ...(componentStack ? ['**Component stack:**', '', '```', componentStack, '```', ''] : []),
      '**Environment:**',
      '',
      `- GitWyrm: ${__APP_VERSION__}`,
      `- Platform: ${navigator.platform}`,
      `- User agent: ${navigator.userAgent}`,
    ].join('\n')
  }

  private handleCopy = () => {
    navigator.clipboard.writeText(this.bugReport()).then(
      () => {
        this.setState({ copied: true })
        setTimeout(() => this.setState({ copied: false }), 2000)
      },
      () => {
        // Clipboard denied (rare in the desktop webview); leave the button as-is.
      }
    )
  }

  render() {
    const { error, info, copied, reporting } = this.state
    if (!error) return this.props.children

    return (
      <div className="flex h-screen flex-col bg-panel text-sm">
        <CrashTitleBar />
        <div className="flex min-h-0 flex-1 flex-col items-center justify-center gap-4 overflow-auto p-8">
        <div className="text-base font-semibold text-removed">Something went wrong</div>
        <div className="max-w-2xl rounded border border-border bg-panel2 p-3 font-mono text-xs text-foreground">
          {error.message}
        </div>
        {info?.componentStack && (
          <pre className="max-h-64 max-w-2xl overflow-auto rounded border border-border bg-panel2 p-3 font-mono text-2xs text-muted-foreground">
            {info.componentStack}
          </pre>
        )}
        <div className="flex items-center gap-2">
          <button
            onClick={this.handleCopy}
            className="rounded border border-border bg-panel3 px-3 py-1.5 text-xs text-foreground hover:border-muted-foreground"
          >
            {copied ? 'Copied!' : 'Copy bug report'}
          </button>
          <button
            onClick={() => this.setState({ reporting: true })}
            className="rounded border border-primary/60 bg-primary/10 px-3 py-1.5 text-xs font-medium text-foreground hover:border-primary"
          >
            Report this problem
          </button>
          <button
            onClick={() => this.setState({ error: null, info: null, copied: false })}
            className="rounded border border-border bg-panel3 px-3 py-1.5 text-xs text-foreground hover:border-muted-foreground"
          >
            Try again
          </button>
        </div>
        </div>

        {/* Prefilled with the crash details so a report needs one click, not a
            retyped description of what the user just saw. */}
        <ReportProblemModal
          open={reporting}
          onClose={() => this.setState({ reporting: false })}
          initialDescription={`GitWyrm crashed with: ${error.message}`}
        />
      </div>
    )
  }
}
