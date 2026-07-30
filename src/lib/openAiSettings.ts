import { emit, listen } from '@tauri-apps/api/event'
import { toast } from 'sonner'
import type { SettingsSection } from '@/stores/uiStore'
import { useUiStore } from '@/stores/uiStore'

const inTauri = '__TAURI_INTERNALS__' in window

/**
 * Ask whichever window owns Settings to show a section.
 *
 * Same shape as the change-selection broadcast in `specSync`: stores are
 * per-window, so an event is the only channel the Desk has to the main window.
 */
const SHOW_SETTINGS_EVENT = 'show-settings-section'

interface ShowSettingsPayload {
  section: SettingsSection
}

/**
 * Open Settings on a section, from either window.
 *
 * In the main window this is a plain navigation. From the Spec Desk -- which has
 * no settings view of its own -- it brings the main window forward and tells it
 * where to go, so "AI settings" in the Desk chip lands the user on the panel
 * rather than on an instruction to go find it.
 *
 * Focus is attempted before the toast, and its failure is not fatal: being told
 * where to go is still useful if the window will not come forward.
 */
export async function openAiSettings(section: SettingsSection = 'ai') {
  if (!inTauri) {
    useUiStore.getState().showSettings(section)
    return
  }

  // The Desk is a separate window; the main one owns Settings.
  const isMain = await isMainWindow()
  if (isMain) {
    useUiStore.getState().showSettings(section)
    return
  }

  void emit(SHOW_SETTINGS_EVENT, { section } satisfies ShowSettingsPayload).catch(() => {
    // The focus change below still gets the user to the right window; landing
    // on the previous section is a smaller failure than not moving at all.
  })

  try {
    const { WebviewWindow } = await import('@tauri-apps/api/webviewWindow')
    const main = await WebviewWindow.getByLabel('main')
    await main?.unminimize()
    await main?.show()
    await main?.setFocus()
  } catch {
    toast.info('Open Settings > AI in the main GitWyrm window.')
  }
}

async function isMainWindow(): Promise<boolean> {
  try {
    const { getCurrentWindow } = await import('@tauri-apps/api/window')
    return getCurrentWindow().label === 'main'
  } catch {
    return true
  }
}

/**
 * Apply settings-section requests broadcast by other windows. Main window only;
 * call once. Returns a teardown function.
 */
export function listenForSettingsRequests(): () => void {
  if (!inTauri) return () => {}
  const stop = listen<ShowSettingsPayload>(SHOW_SETTINGS_EVENT, (event) => {
    useUiStore.getState().showSettings(event.payload.section)
  })
  return () => {
    void stop.then((un) => un()).catch(() => {})
  }
}
