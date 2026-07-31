import { emit, listen } from '@tauri-apps/api/event'

const inTauri = '__TAURI_INTERNALS__' in window

/**
 * Broadcast that settings on disk changed and other windows should re-read them.
 *
 * Same shape as the change-selection broadcast in `specSync`: stores are
 * per-window, so an event is the only channel between them.
 *
 * Lives in its own module rather than beside the store so the store can fire it
 * without importing anything that imports the store back.
 */
const SETTINGS_CHANGED_EVENT = 'settings-changed'

interface SettingsChangedPayload {
  /** Label of the window that wrote, so it can ignore its own echo. */
  from: string
}

/** The current window's label, or '' when it cannot be determined. */
function myLabel(): string {
  try {
    const internals = (
      window as {
        __TAURI_INTERNALS__?: { metadata?: { currentWindow?: { label?: string } } }
      }
    ).__TAURI_INTERNALS__
    return internals?.metadata?.currentWindow?.label ?? ''
  } catch {
    return ''
  }
}

/**
 * Tell other windows their copy of settings is stale.
 *
 * Each webview keeps its own store and writes straight to settings.json, so a
 * provider connected in one window is invisible to the other until it re-reads.
 * That is how the Desk chip could read "not set up" while the main window
 * offered to run a task -- two windows disagreeing about the same state.
 *
 * Fire-and-forget: a dropped event costs a stale panel until the next launch,
 * never a lost setting, because the write itself has already landed on disk.
 */
export function broadcastSettingsChanged() {
  if (!inTauri) return
  void emit(SETTINGS_CHANGED_EVENT, { from: myLabel() } satisfies SettingsChangedPayload).catch(
    () => {}
  )
}

/**
 * Run `onChange` when another window changes settings. Call once per window;
 * returns a teardown function.
 */
export function listenForSettingsChanges(onChange: () => void): () => void {
  if (!inTauri) return () => {}
  const me = myLabel()
  const stop = listen<SettingsChangedPayload>(SETTINGS_CHANGED_EVENT, (event) => {
    // Ignore our own echo: this window's store already holds the newer value,
    // and re-reading would race the write still on its way to disk.
    if (event.payload?.from && event.payload.from === me) return
    onChange()
  })
  return () => {
    void stop.then((un) => un()).catch(() => {})
  }
}
