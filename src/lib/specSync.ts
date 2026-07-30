import { emit, listen } from '@tauri-apps/api/event'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { useUiStore } from '@/stores/uiStore'

const inTauri = '__TAURI_INTERNALS__' in window

/**
 * Which change is selected has to travel between the main window and the Spec
 * Desk, and Zustand stores live per-window (each webview is its own JS context).
 * A Tauri event is the only shared channel, so one small broadcast keeps both
 * windows pointed at the same change.
 */
const SELECT_EVENT = 'spec-change-selected'

interface SelectPayload {
  changeId: string | null
  /** Window that made the change, so it can ignore its own echo. */
  from: string
}

/** Label of this window, used to drop our own broadcasts. */
function myLabel(): string {
  return inTauri ? getCurrentWindow().label : 'web'
}

/**
 * Select a change in every window.
 *
 * Writes locally first so the click feels instant, then broadcasts. The local
 * write is not conditional on the broadcast succeeding: a failed emit must not
 * make a click look ignored.
 */
export function selectChangeEverywhere(changeId: string | null) {
  useUiStore.getState().selectChange(changeId)
  if (!inTauri) return
  void emit(SELECT_EVENT, { changeId, from: myLabel() } satisfies SelectPayload).catch(() => {
    // Selection already applied locally; a dropped broadcast only costs the
    // other window's echo, which its next render or refresh corrects.
  })
}

/**
 * Apply selections broadcast by other windows. Call once per window.
 * Returns a teardown function.
 */
export function listenForSpecSelection(): () => void {
  if (!inTauri) return () => {}
  const me = myLabel()
  const stop = listen<SelectPayload>(SELECT_EVENT, (event) => {
    // Skip our own echo, or the two windows would fight over the store.
    if (event.payload.from === me) return
    useUiStore.getState().selectChange(event.payload.changeId)
  })
  return () => {
    void stop.then((un) => un()).catch(() => {})
  }
}
