import type { SettingsSection } from '@/stores/uiStore'
import { useUiStore } from '@/stores/uiStore'

/**
 * Open AI settings, from any window.
 *
 * A dialog in the window the user is already in. Connecting a provider happens
 * mid-task -- part way through a commit message, or before starting a spec run
 * -- so this deliberately does not change views or windows. It used to pull the
 * user out of the Spec Desk and into the main window, which is the largest
 * possible interruption for pasting an API key.
 *
 * `section` is kept for callers wanting a different panel; anything other than
 * 'ai' still goes to the settings view, which is where those live.
 */
export function openAiSettings(section: SettingsSection = 'ai') {
  if (section !== 'ai') {
    useUiStore.getState().showSettings(section)
    return
  }
  useUiStore.getState().openModal('aiSettings')
}
