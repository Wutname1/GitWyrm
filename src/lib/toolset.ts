/**
 * Keeps the bundled git and gpg current.
 *
 * These used to ship inside the installer, which meant every app update deleted
 * and re-extracted the whole tree - 408 files, 103 MB - for tools that only move
 * when Git for Windows ships, roughly monthly. They come from the CDN now, and
 * this is what notices a new version and fetches it.
 */

import { commands } from '@/lib/bindings'
import { setSplashBar, clearSplashBar } from '@/lib/splash'

/** Event the backend emits toolset download progress on. Matches updates.rs. */
const TOOLSET_PROGRESS_EVENT = 'toolset://progress'

interface ToolsetProgress {
	downloaded: number
	total: number | null
}

function formatMb(bytes: number): string {
	return `${(bytes / 1_000_000).toFixed(1)} MB`
}

/**
 * Bring the toolset up to date, narrating through `onStatus`.
 *
 * Never throws and never blocks the boot on a failure: a missing or stale
 * toolset degrades to "use whatever git is on PATH", which is already the
 * resolution order the backend applies. Someone offline on a machine with git
 * installed should not be held at the splash over this.
 */
export async function ensureToolset(onStatus: (message: string) => void): Promise<void> {
	try {
		const status = await commands.toolsetStatus()
		if (status.status === 'error') return
		if (!status.data.updateAvailable) return

		// First install reads differently from an upgrade: one is "getting
		// ready", the other is routine maintenance the user did not ask for.
		const isFirstInstall = status.data.installed === null
		onStatus(isFirstInstall ? 'Getting git ready' : 'Updating git')

		const { listen } = await import('@tauri-apps/api/event')
		const unlisten = await listen<ToolsetProgress>(TOOLSET_PROGRESS_EVENT, ({ payload }) => {
			const { downloaded, total } = payload
			if (total && total > 0) {
				onStatus(`Downloading git tools ${formatMb(downloaded)} of ${formatMb(total)}`)
				setSplashBar(downloaded / total)
			} else {
				onStatus(`Downloading git tools ${formatMb(downloaded)}`)
				setSplashBar(null)
			}
		})

		try {
			const result = await commands.installToolset()
			if (result.status === 'error') {
				// Logged rather than surfaced: the app still works through system
				// git, and a modal at boot over a background tool update would be
				// far more disruptive than the stale copy it is warning about.
				console.warn('toolset update failed:', result.error)
			}
		} finally {
			unlisten()
			clearSplashBar()
		}
	} catch (e) {
		console.warn('toolset check failed:', e)
	}
}
