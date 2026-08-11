import { beforeEach, describe, expect, it, vi } from 'vitest'

const toolsetStatus = vi.fn()
const installToolset = vi.fn()

vi.mock('@/lib/bindings', () => ({
	commands: {
		toolsetStatus: () => toolsetStatus(),
		installToolset: () => installToolset(),
	},
}))

vi.mock('@/lib/splash', () => ({
	setSplashBar: vi.fn(),
	clearSplashBar: vi.fn(),
}))

vi.mock('@tauri-apps/api/event', () => ({
	listen: vi.fn(async () => () => {}),
}))

/**
 * `ensureToolset` memoizes at module scope, so each case needs a fresh copy of
 * the module rather than a fresh mock.
 */
async function freshEnsureToolset() {
	vi.resetModules()
	const mod = await import('./toolset')
	return mod.ensureToolset
}

describe('ensureToolset', () => {
	beforeEach(() => {
		toolsetStatus.mockReset()
		installToolset.mockReset()
	})

	it('checks the manifest once even when called repeatedly', async () => {
		// A dev hot-reload remounts the tree without restarting the backend, and
		// the launch effect's own guard resets with it. Without memoizing, each
		// remount would re-check - and once a manifest exists, re-download.
		toolsetStatus.mockResolvedValue({
			status: 'ok',
			data: { installed: 'v1', available: 'v1', updateAvailable: false },
		})
		const ensureToolset = await freshEnsureToolset()

		await ensureToolset(() => {})
		await ensureToolset(() => {})
		await ensureToolset(() => {})

		expect(toolsetStatus).toHaveBeenCalledTimes(1)
	})

	it('does not start a second download while the first is running', async () => {
		// The case that actually costs something: two overlapping installs would
		// unpack 23 MB into the same scratch directory and race into the swap.
		toolsetStatus.mockResolvedValue({
			status: 'ok',
			data: { installed: null, available: 'v2', updateAvailable: true },
		})
		let release: () => void = () => {}
		installToolset.mockReturnValue(
			new Promise((resolve) => {
				release = () => resolve({ status: 'ok', data: 'v2' })
			}),
		)
		const ensureToolset = await freshEnsureToolset()

		const first = ensureToolset(() => {})
		const second = ensureToolset(() => {})
		release()
		await Promise.all([first, second])

		expect(installToolset).toHaveBeenCalledTimes(1)
	})

	it('installs when the manifest offers a newer version', async () => {
		toolsetStatus.mockResolvedValue({
			status: 'ok',
			data: { installed: 'v1', available: 'v2', updateAvailable: true },
		})
		installToolset.mockResolvedValue({ status: 'ok', data: 'v2' })
		const ensureToolset = await freshEnsureToolset()

		await ensureToolset(() => {})

		expect(installToolset).toHaveBeenCalledTimes(1)
	})

	it('skips the download when the toolset is already current', async () => {
		toolsetStatus.mockResolvedValue({
			status: 'ok',
			data: { installed: 'v2', available: 'v2', updateAvailable: false },
		})
		const ensureToolset = await freshEnsureToolset()

		await ensureToolset(() => {})

		expect(installToolset).not.toHaveBeenCalled()
	})

	it('never throws when the manifest cannot be reached', async () => {
		// Offline with git on PATH must still reach the repositories: the backend
		// already falls back to a system install, so this must not block the boot.
		toolsetStatus.mockRejectedValue(new Error('offline'))
		const ensureToolset = await freshEnsureToolset()

		await expect(ensureToolset(() => {})).resolves.toBeUndefined()
		expect(installToolset).not.toHaveBeenCalled()
	})

	it('never throws when the install itself fails', async () => {
		toolsetStatus.mockResolvedValue({
			status: 'ok',
			data: { installed: null, available: 'v2', updateAvailable: true },
		})
		installToolset.mockResolvedValue({ status: 'error', error: 'checksum mismatch' })
		const ensureToolset = await freshEnsureToolset()

		await expect(ensureToolset(() => {})).resolves.toBeUndefined()
	})
})
