import { beforeEach, describe, expect, it, vi } from 'vitest'
import { DEFAULT_AGENT_WORKSPACE_LAYOUT, MIN_DOCK_SIZE_PX } from '@/lib/agentWorkspaceLayout'

/**
 * The store is module state (a debounced `persistTimer` module variable plus
 * the zustand store itself), so each test re-imports a fresh copy the same
 * way `avatarSource.test.ts` does for its module-level cache -- otherwise a
 * pending timer or a stale `hydrated` flag from a previous test leaks in.
 */
type Mod = typeof import('./agentDeskUiStore')
let useAgentDeskUiStore: Mod['useAgentDeskUiStore']
let flushPendingAgentDeskLayout: Mod['flushPendingAgentDeskLayout']

/** Enough of the Storage API to exercise the store; the suite runs headless (node env, no DOM). */
function fakeStorage(): Storage {
  const data = new Map<string, string>()
  return {
    getItem: (k: string) => data.get(k) ?? null,
    setItem: (k: string, v: string) => void data.set(k, v),
    removeItem: (k: string) => void data.delete(k),
    clear: () => data.clear(),
    key: (i: number) => [...data.keys()][i] ?? null,
    get length() {
      return data.size
    },
  } as Storage
}

let storage: Storage

beforeEach(async () => {
  vi.useFakeTimers()
  storage = fakeStorage()
  vi.stubGlobal('localStorage', storage)
  vi.resetModules()
  ;({ useAgentDeskUiStore, flushPendingAgentDeskLayout } = await import('./agentDeskUiStore'))
})

describe('agentDeskUiStore: hydration and persistence', () => {
  it('starts unhydrated with the default layout before hydrate() runs', () => {
    const s = useAgentDeskUiStore.getState()
    expect(s.hydrated).toBe(false)
    expect(s.layout).toEqual(DEFAULT_AGENT_WORKSPACE_LAYOUT)
  })

  it('does not persist changes made before hydrate() runs', () => {
    useAgentDeskUiStore.getState().setSourceBarsVisible(false)
    vi.advanceTimersByTime(1000)
    expect(storage.getItem('gitwyrm.agentDeskWorkspaceLayout.v1')).toBeNull()
  })

  it('persists a layout change after hydration, debounced', () => {
    useAgentDeskUiStore.getState().hydrate()
    useAgentDeskUiStore.getState().setSourceBarsVisible(false)
    // Not yet written -- debounce window has not elapsed.
    expect(storage.getItem('gitwyrm.agentDeskWorkspaceLayout.v1')).toBeNull()
    vi.advanceTimersByTime(300)
    const raw = storage.getItem('gitwyrm.agentDeskWorkspaceLayout.v1')
    expect(raw).not.toBeNull()
    const parsed = JSON.parse(raw as string)
    expect(parsed.schemaVersion).toBe(1)
    expect(parsed.layout.sourceBarsVisible).toBe(false)
  })

  it('round-trips a real layout through hydrate() after a previous session persisted it', () => {
    useAgentDeskUiStore.getState().hydrate()
    useAgentDeskUiStore.getState().openSplit()
    useAgentDeskUiStore.getState().setPaneSession('primary', 'session-a')
    useAgentDeskUiStore.getState().setPaneSession('secondary', 'session-b')
    useAgentDeskUiStore.getState().setActivePane('secondary')
    vi.advanceTimersByTime(300)

    // Simulate a fresh launch: reset the module state and re-hydrate from the
    // same fake storage.
    vi.resetModules()
    return import('./agentDeskUiStore').then(({ useAgentDeskUiStore: fresh }) => {
      fresh.getState().hydrate()
      const s = fresh.getState()
      expect(s.layout.split).toBe(true)
      expect(s.layout.primarySessionId).toBe('session-a')
      expect(s.layout.secondarySessionId).toBe('session-b')
      expect(s.layout.activePane).toBe('secondary')
    })
  })

  it('falls back safely to defaults when the persisted payload is malformed', () => {
    storage.setItem('gitwyrm.agentDeskWorkspaceLayout.v1', '{"not":"a real layout"}')
    useAgentDeskUiStore.getState().hydrate()
    expect(useAgentDeskUiStore.getState().layout).toEqual(DEFAULT_AGENT_WORKSPACE_LAYOUT)
    expect(useAgentDeskUiStore.getState().hydrated).toBe(true)
  })

  it('falls back safely to defaults when the persisted payload is not even JSON', () => {
    storage.setItem('gitwyrm.agentDeskWorkspaceLayout.v1', 'not json at all {{{')
    expect(() => useAgentDeskUiStore.getState().hydrate()).not.toThrow()
    expect(useAgentDeskUiStore.getState().layout).toEqual(DEFAULT_AGENT_WORKSPACE_LAYOUT)
    expect(useAgentDeskUiStore.getState().hydrated).toBe(true)
  })

  it('migrates an older-but-currently-identical version cleanly on restore', () => {
    storage.setItem(
      'gitwyrm.agentDeskWorkspaceLayout.v1',
      JSON.stringify({ schemaVersion: 1, layout: { ...DEFAULT_AGENT_WORKSPACE_LAYOUT, sourceBarsVisible: false } })
    )
    useAgentDeskUiStore.getState().hydrate()
    expect(useAgentDeskUiStore.getState().layout.sourceBarsVisible).toBe(false)
  })

  it('rejects a future schema version and falls back to defaults rather than misreading it', () => {
    storage.setItem(
      'gitwyrm.agentDeskWorkspaceLayout.v1',
      JSON.stringify({ schemaVersion: 999, layout: { ...DEFAULT_AGENT_WORKSPACE_LAYOUT, sourceBarsVisible: false } })
    )
    useAgentDeskUiStore.getState().hydrate()
    // Must NOT pick up sourceBarsVisible: false from the future payload.
    expect(useAgentDeskUiStore.getState().layout).toEqual(DEFAULT_AGENT_WORKSPACE_LAYOUT)
  })

  it('clamps a restored dock size to the current window bound', () => {
    storage.setItem(
      'gitwyrm.agentDeskWorkspaceLayout.v1',
      JSON.stringify({
        schemaVersion: 1,
        layout: { ...DEFAULT_AGENT_WORKSPACE_LAYOUT, dock: { kind: 'source', edge: 'right', sizePx: 900 } },
      })
    )
    useAgentDeskUiStore.getState().hydrate(500)
    const dock = useAgentDeskUiStore.getState().layout.dock
    expect(dock).not.toBeNull()
    expect(dock?.sizePx).toBeLessThan(500)
    expect(dock?.sizePx).toBeGreaterThanOrEqual(MIN_DOCK_SIZE_PX)
  })

  it('flushPendingAgentDeskLayout writes immediately without waiting for the debounce', () => {
    useAgentDeskUiStore.getState().hydrate()
    useAgentDeskUiStore.getState().setSourceBarsVisible(false)
    expect(storage.getItem('gitwyrm.agentDeskWorkspaceLayout.v1')).toBeNull()
    flushPendingAgentDeskLayout()
    const raw = storage.getItem('gitwyrm.agentDeskWorkspaceLayout.v1')
    expect(raw).not.toBeNull()
    expect(JSON.parse(raw as string).layout.sourceBarsVisible).toBe(false)
  })
})

describe('agentDeskUiStore: pane replacement and split view', () => {
  beforeEach(() => {
    useAgentDeskUiStore.getState().hydrate()
  })

  it('replacing a pane session does not change the active pane', () => {
    useAgentDeskUiStore.getState().setActivePane('secondary')
    useAgentDeskUiStore.getState().setPaneSession('primary', 'session-a')
    expect(useAgentDeskUiStore.getState().layout.activePane).toBe('secondary')
    expect(useAgentDeskUiStore.getState().layout.primarySessionId).toBe('session-a')
  })

  it('collapsing split with the primary pane active keeps the primary session', () => {
    useAgentDeskUiStore.getState().setPaneSession('primary', 'session-a')
    useAgentDeskUiStore.getState().setPaneSession('secondary', 'session-b')
    useAgentDeskUiStore.getState().openSplit()
    useAgentDeskUiStore.getState().setActivePane('primary')
    useAgentDeskUiStore.getState().closeSplit()
    const s = useAgentDeskUiStore.getState().layout
    expect(s.split).toBe(false)
    expect(s.primarySessionId).toBe('session-a')
    expect(s.secondarySessionId).toBeNull()
  })

  it('collapsing split with the secondary pane active promotes it to the sole pane (spec 5.6)', () => {
    useAgentDeskUiStore.getState().setPaneSession('primary', 'session-a')
    useAgentDeskUiStore.getState().setPaneSession('secondary', 'session-b')
    useAgentDeskUiStore.getState().openSplit()
    useAgentDeskUiStore.getState().setActivePane('secondary')
    useAgentDeskUiStore.getState().closeSplit()
    const s = useAgentDeskUiStore.getState().layout
    expect(s.split).toBe(false)
    expect(s.activePane).toBe('primary')
    // The promoted session -- what was showing in the right pane -- is now
    // the single visible conversation.
    expect(s.primarySessionId).toBe('session-b')
    expect(s.secondarySessionId).toBeNull()
  })

  it('restorePaneToFallback replaces a missing session with the given fallback', () => {
    useAgentDeskUiStore.getState().setPaneSession('primary', 'deleted-session')
    useAgentDeskUiStore.getState().restorePaneToFallback('primary', 'newest-valid-session')
    expect(useAgentDeskUiStore.getState().layout.primarySessionId).toBe('newest-valid-session')
  })

  // R4.3: the layout has never stored a repo -- only a session ID per pane --
  // so nothing here needs to change to let Split View show two different
  // repositories at once. This test exists to make that guarantee explicit
  // rather than only implied by the shape of `AgentWorkspaceLayout`.
  it('holds one session per pane with no repo concept, so Split View can show sessions from different repos', () => {
    useAgentDeskUiStore.getState().openSplit()
    useAgentDeskUiStore.getState().setPaneSession('primary', 'repo-a-session')
    useAgentDeskUiStore.getState().setPaneSession('secondary', 'repo-b-session')
    const s = useAgentDeskUiStore.getState().layout
    expect(s.primarySessionId).toBe('repo-a-session')
    expect(s.secondarySessionId).toBe('repo-b-session')
    expect(s).not.toHaveProperty('repoId')
  })
})

describe('agentDeskUiStore: dock', () => {
  beforeEach(() => {
    useAgentDeskUiStore.getState().hydrate()
  })

  it('openDock only carries leftOrder for the left edge', () => {
    useAgentDeskUiStore.getState().openDock('context', 'right', 'above-chats')
    expect(useAgentDeskUiStore.getState().layout.dock).toEqual({ kind: 'context', edge: 'right', sizePx: expect.any(Number) })
  })

  it('moveDock drops leftOrder when moving away from left', () => {
    useAgentDeskUiStore.getState().openDock('context', 'left', 'below-chats')
    useAgentDeskUiStore.getState().moveDock('bottom')
    const dock = useAgentDeskUiStore.getState().layout.dock
    expect(dock?.edge).toBe('bottom')
    expect(dock?.leftOrder).toBeUndefined()
  })

  it('resizeDock clamps to the safe range', () => {
    useAgentDeskUiStore.getState().openDock('source', 'right')
    useAgentDeskUiStore.getState().resizeDock(5)
    expect(useAgentDeskUiStore.getState().layout.dock?.sizePx).toBe(MIN_DOCK_SIZE_PX)
  })

  it('closeDock clears the dock entirely', () => {
    useAgentDeskUiStore.getState().openDock('graph', 'bottom')
    useAgentDeskUiStore.getState().closeDock()
    expect(useAgentDeskUiStore.getState().layout.dock).toBeNull()
  })

  it('resetLayout restores every field to the default', () => {
    useAgentDeskUiStore.getState().openSplit()
    useAgentDeskUiStore.getState().openDock('graph', 'bottom')
    useAgentDeskUiStore.getState().setSourceBarsVisible(false)
    useAgentDeskUiStore.getState().resetLayout()
    expect(useAgentDeskUiStore.getState().layout).toEqual(DEFAULT_AGENT_WORKSPACE_LAYOUT)
  })
})

describe('agentDeskUiStore: session-scoped composer drafts', () => {
  beforeEach(() => {
    useAgentDeskUiStore.getState().hydrate()
  })

  it('preserves a draft when a different chat replaces its pane', () => {
    useAgentDeskUiStore.getState().setDraft('session-a', 'unsent text for A')
    useAgentDeskUiStore.getState().setPaneSession('primary', 'session-a')
    // Replace the pane with a different session -- the pane state changes,
    // but nothing about the draft map should.
    useAgentDeskUiStore.getState().setPaneSession('primary', 'session-b')
    expect(useAgentDeskUiStore.getState().getDraft('session-a')).toBe('unsent text for A')
  })

  it('restores the exact draft when that session is selected again', () => {
    useAgentDeskUiStore.getState().setDraft('session-a', 'first draft')
    useAgentDeskUiStore.getState().setPaneSession('primary', 'session-b')
    useAgentDeskUiStore.getState().setPaneSession('primary', 'session-a')
    expect(useAgentDeskUiStore.getState().getDraft('session-a')).toBe('first draft')
  })

  it('keeps primary and secondary session drafts independent', () => {
    useAgentDeskUiStore.getState().openSplit()
    useAgentDeskUiStore.getState().setPaneSession('primary', 'session-a')
    useAgentDeskUiStore.getState().setPaneSession('secondary', 'session-b')
    useAgentDeskUiStore.getState().setDraft('session-a', 'primary text')
    useAgentDeskUiStore.getState().setDraft('session-b', 'secondary text')
    expect(useAgentDeskUiStore.getState().getDraft('session-a')).toBe('primary text')
    expect(useAgentDeskUiStore.getState().getDraft('session-b')).toBe('secondary text')
  })

  it('clears only the accepted session draft after Send persists its user event', () => {
    useAgentDeskUiStore.getState().setDraft('session-a', 'text a')
    useAgentDeskUiStore.getState().setDraft('session-b', 'text b')
    useAgentDeskUiStore.getState().clearDraft('session-a')
    expect(useAgentDeskUiStore.getState().getDraft('session-a')).toBe('')
    expect(useAgentDeskUiStore.getState().getDraft('session-b')).toBe('text b')
  })

  it('preserves a draft when Send fails -- clearDraft is only called on accepted Send, so a caller that skips it on failure keeps the text', () => {
    useAgentDeskUiStore.getState().setDraft('session-a', 'about to fail')
    // Simulated failed Send: the caller never calls clearDraft.
    expect(useAgentDeskUiStore.getState().getDraft('session-a')).toBe('about to fail')
  })

  it('an unknown session has no draft', () => {
    expect(useAgentDeskUiStore.getState().getDraft('never-touched')).toBe('')
  })

  it('handles rapid switching between many sessions without cross-talk', () => {
    const store = useAgentDeskUiStore.getState()
    for (let i = 0; i < 50; i++) {
      store.setPaneSession('primary', `session-${i}`)
      store.setDraft(`session-${i}`, `draft ${i}`)
    }
    for (let i = 0; i < 50; i++) {
      expect(useAgentDeskUiStore.getState().getDraft(`session-${i}`)).toBe(`draft ${i}`)
    }
  })

  it('drafts are never written to localStorage', () => {
    useAgentDeskUiStore.getState().setDraft('session-a', 'sensitive unsent prompt')
    vi.advanceTimersByTime(1000)
    flushPendingAgentDeskLayout()
    const raw = storage.getItem('gitwyrm.agentDeskWorkspaceLayout.v1')
    expect(raw === null || !raw.includes('sensitive unsent prompt')).toBe(true)
  })
})
