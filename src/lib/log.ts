import { debug, info, warn, error as logError } from '@tauri-apps/plugin-log'

/**
 * Frontend logging bridge.
 *
 * Every message here is forwarded to the Rust `tauri-plugin-log`, which writes
 * it to `gitwyrm.log` alongside the backend's own entries. Before this existed,
 * frontend failures only ever became toasts and never reached the log file --
 * so a user reporting "there's an error toast but nothing in the logs" had no
 * durable trace to hand us. Now they do.
 *
 * These forward fire-and-forget: logging must never throw into the code path it
 * is observing, so a failed log call is swallowed.
 */

function forward(fn: (msg: string) => Promise<void>, msg: string) {
  fn(msg).catch(() => {
    // A logger that throws is worse than one that drops a line. Swallow it.
  })
}

export const log = {
  debug: (msg: string) => forward(debug, msg),
  info: (msg: string) => forward(info, msg),
  warn: (msg: string) => forward(warn, msg),
  error: (msg: string) => forward(logError, msg),
}

/** Serialize an unknown thrown value into a single log-friendly line. */
export function describeError(e: unknown): string {
  if (e instanceof Error) return e.stack ?? `${e.name}: ${e.message}`
  if (typeof e === 'string') return e
  try {
    return JSON.stringify(e)
  } catch {
    return String(e)
  }
}
