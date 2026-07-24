import { create } from 'zustand'
import { toast } from 'sonner'
import { useWorkspaceStore } from '@/stores/workspaceStore'

export type UpdateState = 'idle' | 'checking' | 'available' | 'downloading' | 'ready' | 'none' | 'error'

interface UpdaterStore {
  state: UpdateState
  /** Version string of the pending update, once one is found. */
  version: string | null
  /**
   * Look for a newer release without installing it. On success, moves to
   * 'available' (with `version` set) or 'none'. `silent` suppresses the
   * toasts, for the automatic check that runs on launch.
   */
  check: (silent?: boolean) => Promise<void>
  /** Download, install, and relaunch into the update found by `check`. */
  install: () => Promise<void>
  /** Check and, if an update exists, install it in one shot (manual trigger). */
  checkAndInstall: () => Promise<void>
  /**
   * Begin re-checking for updates every `intervalMs`. Returns a cleanup that
   * stops the timer. Once an update is found the timer stops on its own -- the
   * status-bar button is showing, so there is nothing more to look for.
   */
  startAutoCheck: (intervalMs: number) => () => void
}

/** How often to look for a newer release while the app stays open: 6 hours. */
export const AUTO_CHECK_INTERVAL_MS = 6 * 60 * 60 * 1000

// Dev builds carry the placeholder version from tauri.conf.json, so every
// release looks newer and would offer to replace the build being worked on.
function skipInDev(silent: boolean, set: (s: Partial<UpdaterStore>) => void): boolean {
  if (!import.meta.env.DEV) return false
  set({ state: 'none' })
  if (!silent) toast('Update checks are off in development builds')
  return true
}

async function runInstall(
  update: { version: string; downloadAndInstall: () => Promise<void> },
  set: (s: Partial<UpdaterStore>) => void,
) {
  set({ state: 'downloading', version: update.version })
  await update.downloadAndInstall()
  set({ state: 'ready' })
  toast(`Update ${update.version} installed - restarting...`)
  const { relaunch } = await import('@tauri-apps/plugin-process')
  await relaunch()
}

async function fetchUpdate() {
  const { check } = await import('@tauri-apps/plugin-updater')
  const channel = useWorkspaceStore.getState().updateChannel
  return check({ headers: { 'X-Update-Channel': channel } })
}

/** Shared updater state so a launch check and the status-bar button agree. */
export const useUpdater = create<UpdaterStore>((set, get) => ({
  state: 'idle',
  version: null,

  check: async (silent = false) => {
    if (skipInDev(silent, set)) return
    // Don't stomp an install already under way.
    if (get().state === 'downloading') return
    set({ state: 'checking' })
    try {
      const update = await fetchUpdate()
      if (!update) {
        set({ state: 'none', version: null })
        if (!silent) toast('GitWyrm is up to date')
        return
      }
      set({ state: 'available', version: update.version })
      if (!silent) toast(`Update ${update.version} is available`)
    } catch (e) {
      set({ state: 'error' })
      if (!silent) toast.error(`Update check failed: ${(e as Error).message}`)
    }
  },

  install: async () => {
    if (skipInDev(false, set)) return
    if (get().state === 'downloading') return
    set({ state: 'checking' })
    try {
      const update = await fetchUpdate()
      if (!update) {
        set({ state: 'none', version: null })
        toast('GitWyrm is up to date')
        return
      }
      await runInstall(update, set)
    } catch (e) {
      set({ state: 'error' })
      toast.error(`Update failed: ${(e as Error).message}`)
    }
  },

  checkAndInstall: async () => {
    await get().install()
  },

  startAutoCheck: (intervalMs) => {
    const timer = setInterval(() => {
      const { state, check } = get()
      // Nothing to re-check once an update is already found or installing, and
      // don't stack a check on top of one still running.
      if (state === 'checking' || state === 'downloading' || state === 'ready' || state === 'available') {
        return
      }
      void check(true)
    }, intervalMs)
    return () => clearInterval(timer)
  },
}))
