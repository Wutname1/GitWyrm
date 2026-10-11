import type { Severity } from '@/lib/errorClass'

/**
 * An error a backend command returned.
 *
 * The backend logs every command error itself, at the IPC boundary
 * (`impl Serialize for AppError` in `src-tauri/src/error.rs`), and that log line
 * is what files a crash report. Logging the same failure again here at `error`
 * filed it a second time - every failure arrived in the reports twice, once as
 * `Command failed: ...` and once as `mutation failed [error]: ...`. Marking the
 * errors that came from the backend lets the frontend keep its log line for the
 * trail without reporting the failure again.
 */
export class CommandError extends Error {
  constructor(message: string) {
    super(message)
    this.name = 'CommandError'
  }
}

export type LogLevel = 'info' | 'warn' | 'error'

/**
 * The level to log a failed action at. A backend failure is never logged above
 * `warn` here, because the backend already reported it; a failure that started
 * in the frontend keeps its severity, since nothing else will report it.
 */
export function failureLogLevel(severity: Severity, e: unknown): LogLevel {
  if (severity === 'info') return 'info'
  if (severity === 'warning') return 'warn'
  return e instanceof CommandError ? 'warn' : 'error'
}
