import { useState } from 'react'
import { toast } from 'sonner'
import { useWorkspaceStore } from '@/stores/workspaceStore'

export type UpdateState = 'idle' | 'checking' | 'available' | 'downloading' | 'ready' | 'none' | 'error'

/** Checks GitHub releases via the Tauri updater and applies updates on demand. */
export function useUpdater() {
  const [state, setState] = useState<UpdateState>('idle')
  const [version, setVersion] = useState<string | null>(null)

  const checkAndInstall = async () => {
    setState('checking')
    try {
      const { check } = await import('@tauri-apps/plugin-updater')
      const channel = useWorkspaceStore.getState().updateChannel
      const update = await check({ headers: { 'X-Update-Channel': channel } })
      if (!update) {
        setState('none')
        toast('GitWyrm is up to date')
        return
      }
      setVersion(update.version)
      setState('downloading')
      await update.downloadAndInstall()
      setState('ready')
      toast(`Update ${update.version} installed — restarting…`)
      const { relaunch } = await import('@tauri-apps/plugin-process')
      await relaunch()
    } catch (e) {
      setState('error')
      toast.error(`Update check failed: ${(e as Error).message}`)
    }
  }

  return { state, version, checkAndInstall }
}
