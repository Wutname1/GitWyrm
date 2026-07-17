/**
 * True when running inside the Tauri webview (IPC available). False in a plain
 * browser like the Vite dev preview, where `commands.*` invocations reject.
 * Use to disable Tauri-backed queries so the preview renders instead of erroring.
 */
export const isTauri = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window
