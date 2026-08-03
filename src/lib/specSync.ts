import { emit, listen } from '@tauri-apps/api/event'
import { getCurrentWindow } from '@tauri-apps/api/window'
import type { QueryClient } from '@tanstack/react-query'
import { invalidateOpenspec } from '@/lib/queryKeys'
import { useUiStore } from '@/stores/uiStore'

const inTauri = '__TAURI_INTERNALS__' in window

/**
 * Which change is selected has to travel between the main window and the Spec
 * Desk, and Zustand stores live per-window (each webview is its own JS context).
 * A Tauri event is the only shared channel, so one small broadcast keeps both
 * windows pointed at the same change.
 */
const SELECT_EVENT = 'spec-change-selected'

/**
 * A window has re-read the spec files and wants the others to do the same.
 *
 * Separate from the selection event because it carries no state: the files on
 * disk are the state, and this only says "look again".
 */
const REFRESH_EVENT = 'spec-refresh-requested'

interface SelectPayload {
  changeId: string | null
  /** Window that made the change, so it can ignore its own echo. */
  from: string
}

interface RefreshPayload {
  /** Repo whose specs to re-read, so a window on another repo ignores it. */
  repoId: string
  /** Window that asked, so it can ignore its own echo. */
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
 * Re-read a repo's spec files in every window.
 *
 * Rewriting history moves the `openspec/` folder underneath both windows, and
 * each webview holds its own query cache, so refreshing locally would leave the
 * other window showing changes that are no longer on disk. Local first, then
 * broadcast, for the same reason selection does it: the click has to look
 * answered even if the emit fails.
 */
export function refreshSpecsEverywhere(qc: QueryClient, repoId: string) {
  invalidateOpenspec(qc, repoId)
  if (!inTauri) return
  void emit(REFRESH_EVENT, { repoId, from: myLabel() } satisfies RefreshPayload).catch(() => {
    // Our own window is already re-reading; the other one still has its own
    // refresh button.
  })
}

/**
 * Re-read when another window asks. Call once per window, with a getter for the
 * repo this window is showing -- the Desk opens its repo asynchronously, so the
 * id is not known when the listener is set up.
 *
 * Returns a teardown function.
 */
export function listenForSpecRefresh(qc: QueryClient, currentRepoId: () => string | null): () => void {
  if (!inTauri) return () => {}
  const me = myLabel()
  const stop = listen<RefreshPayload>(REFRESH_EVENT, (event) => {
    if (event.payload.from === me) return
    // A window showing a different repo has nothing to re-read here.
    if (event.payload.repoId !== currentRepoId()) return
    invalidateOpenspec(qc, event.payload.repoId)
  })
  return () => {
    void stop.then((un) => un()).catch(() => {})
  }
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
