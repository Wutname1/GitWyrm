import { describe, it, expect, beforeEach } from 'vitest'

// Same reasoning as workspaceStore.submodules.test.ts: these run in the `node`
// environment, and settingsSync reads `window` at module scope to decide whether
// it is inside Tauri. An empty object answers that correctly and costs nothing.
;(globalThis as { window?: unknown }).window ??= {}

const { useWorkspaceStore } = await import('./workspaceStore')

const RENAMED = 'C:\\code\\old-name'
const KEPT = 'C:\\code\\other'

/** `hydrated: false` stops schedulePersist from reaching for a backend. */
beforeEach(() => {
  useWorkspaceStore.setState({
    recents: [
      { name: 'old-name', path: RENAMED },
      { name: 'other', path: KEPT },
    ],
    hydrated: false,
  })
})

describe('removeRecent', () => {
  /**
   * The reported case: a repo renamed on disk leaves a recent row pointing at a
   * path that no longer exists, and there was no way to get rid of it.
   */
  it('drops the named entry and leaves the rest in order', () => {
    useWorkspaceStore.getState().removeRecent(RENAMED)

    const { recents } = useWorkspaceStore.getState()
    expect(recents.map((r) => r.path)).toEqual([KEPT])
  })

  /**
   * A recent row is a bookmark. Removing one must not touch the repo, which is
   * the whole reason this is safe to offer for a path that still exists.
   */
  it('leaves open repositories alone', () => {
    const before = useWorkspaceStore.getState().openRepos
    useWorkspaceStore.getState().removeRecent(RENAMED)
    expect(useWorkspaceStore.getState().openRepos).toBe(before)
  })

  /**
   * Paths reach the store in whatever slash style the user typed, so matching
   * has to go through samePath rather than string equality -- otherwise the
   * button appears to do nothing for the exact rows it exists to clear.
   */
  it('matches regardless of slash style or trailing separator', () => {
    useWorkspaceStore.getState().removeRecent('C:/code/old-name/')

    expect(useWorkspaceStore.getState().recents.map((r) => r.path)).toEqual([
      KEPT,
    ])
  })

  /**
   * Removing something already gone must not disturb the list: returning a new
   * array would re-render every consumer for no change.
   */
  it('is a no-op for a path that is not listed', () => {
    const before = useWorkspaceStore.getState().recents
    useWorkspaceStore.getState().removeRecent('C:\\code\\never-seen')
    expect(useWorkspaceStore.getState().recents).toBe(before)
  })
})
