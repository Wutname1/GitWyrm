/**
 * Keeps git and gpg current.
 *
 * These used to ship inside the installer, which meant every app update deleted
 * and re-extracted the whole tree - 408 files, 103 MB - for tools that only move
 * when Git for Windows ships, roughly monthly. They come from the CDN now.
 *
 * **The first install is the bootstrapper's job**, not this one: GitWyrm-Setup
 * fetches the tools right after installing the app, so a clean machine has them
 * before the app ever starts. This is the update half - it notices a newer
 * version and fetches it - and doubles as the safety net for the two cases the
 * bootstrapper cannot cover: someone who ran the NSIS installer directly from
 * the GitHub release, and an install-time download that failed.
 */

import { commands } from '@/lib/bindings'
import { setSplashBar, clearSplashBar } from '@/lib/splash'

/** Event the backend emits toolset download progress on. Matches updates.rs. */
const TOOLSET_PROGRESS_EVENT = 'toolset://progress'

/**
 * The in-flight (or finished) check, so this only ever runs once per process.
 *
 * Module-level rather than a component ref: a dev hot-reload remounts the tree
 * without restarting the backend, and the launch effect's own guard resets with
 * it. Two overlapping calls would mean two 23 MB downloads unpacking into the
 * same scratch directory, racing each other into the swap.
 */
let inFlight: Promise<void> | null = null

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
	// Later callers await the first call rather than starting their own.
	// Deliberately never reset: a second check within one process has nothing
	// new to find, and the cost of being wrong is a duplicate 23 MB download.
	inFlight ??= runEnsureToolset(onStatus)
	return inFlight
}

async function runEnsureToolset(onStatus: (message: string) => void): Promise<void> {
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
