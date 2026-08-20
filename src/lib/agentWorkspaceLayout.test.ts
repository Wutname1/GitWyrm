import { describe, expect, it } from 'vitest'
import {
  CURRENT_LAYOUT_SCHEMA_VERSION,
  DEFAULT_AGENT_WORKSPACE_LAYOUT,
  MAX_DOCK_SIZE_PX,
  MIN_DOCK_SIZE_PX,
  clampDockSizePx,
  defaultConversationPaneState,
  migrateLayout,
  toPersistedLayout,
  type AgentWorkspaceLayout,
} from './agentWorkspaceLayout'

const REAL_LAYOUT: AgentWorkspaceLayout = {
  split: true,
  activePane: 'secondary',
  primarySessionId: 'session-a',
  secondarySessionId: 'session-b',
  sourceBarsVisible: false,
  dock: { kind: 'context', edge: 'left', leftOrder: 'below-chats', sizePx: 420 },
}

describe('migrateLayout', () => {
  it('round-trips a valid current-version layout unchanged', () => {
    const persisted = toPersistedLayout(REAL_LAYOUT)
    const result = migrateLayout(persisted)
    expect(result).toEqual({ status: 'ok', layout: REAL_LAYOUT })
  })

  it('round-trips the default layout (dock: null, no leftOrder leakage)', () => {
    const persisted = toPersistedLayout(DEFAULT_AGENT_WORKSPACE_LAYOUT)
    const result = migrateLayout(persisted)
    expect(result).toEqual({ status: 'ok', layout: DEFAULT_AGENT_WORKSPACE_LAYOUT })
  })

  it('falls back safely on null', () => {
    const result = migrateLayout(null)
    expect(result).toEqual({ status: 'fallback', layout: DEFAULT_AGENT_WORKSPACE_LAYOUT, reason: 'malformed' })
  })

  it('falls back safely on a non-object', () => {
    expect(migrateLayout('nonsense')).toEqual({
      status: 'fallback',
      layout: DEFAULT_AGENT_WORKSPACE_LAYOUT,
      reason: 'malformed',
    })
    expect(migrateLayout(42)).toEqual({
      status: 'fallback',
      layout: DEFAULT_AGENT_WORKSPACE_LAYOUT,
      reason: 'malformed',
    })
  })

  it('falls back safely when schemaVersion is missing', () => {
    const result = migrateLayout({ layout: REAL_LAYOUT })
    expect(result).toEqual({ status: 'fallback', layout: DEFAULT_AGENT_WORKSPACE_LAYOUT, reason: 'malformed' })
  })

  it('falls back safely when layout is missing or malformed at the current version', () => {
    expect(migrateLayout({ schemaVersion: 1 })).toEqual({
      status: 'fallback',
      layout: DEFAULT_AGENT_WORKSPACE_LAYOUT,
      reason: 'malformed',
    })
    expect(migrateLayout({ schemaVersion: 1, layout: 'nope' })).toEqual({
      status: 'fallback',
      layout: DEFAULT_AGENT_WORKSPACE_LAYOUT,
      reason: 'malformed',
    })
  })

  it('substitutes field-level defaults for individually malformed fields rather than discarding the whole layout', () => {
    const result = migrateLayout({
      schemaVersion: 1,
      layout: {
        split: 'yes', // wrong type
        activePane: 'tertiary', // not a real pane
        primarySessionId: 123, // wrong type
        secondarySessionId: null,
        sourceBarsVisible: 1, // wrong type
        dock: { kind: 'bogus', edge: 'left', sizePx: 400 }, // invalid kind
      },
    })
    expect(result).toEqual({ status: 'ok', layout: DEFAULT_AGENT_WORKSPACE_LAYOUT })
  })

  it('migrates an older-but-known version (v1 is currently the floor, so this is the identity case)', () => {
    // There is no predecessor to v1 yet; this documents that a version equal
    // to CURRENT_LAYOUT_SCHEMA_VERSION migrates cleanly, exactly the
    // `migrate_session_accepts_the_current_schema_version` case in
    // src-tauri/src/agentdesk/model.rs. When a v2 ships, this test should
    // gain a real "v1 payload upgrades to v2 shape" sibling.
    expect(CURRENT_LAYOUT_SCHEMA_VERSION).toBe(1)
    const result = migrateLayout({ schemaVersion: 1, layout: REAL_LAYOUT })
    expect(result.status).toBe('ok')
  })

  it('rejects a schema version from the future rather than misreading it', () => {
    const result = migrateLayout({ schemaVersion: 99, layout: REAL_LAYOUT })
    expect(result).toEqual({
      status: 'fallback',
      layout: DEFAULT_AGENT_WORKSPACE_LAYOUT,
      reason: 'unsupported-schema-version',
    })
  })

  it('rejects a schema version below 1 (no known migration path)', () => {
    const result = migrateLayout({ schemaVersion: 0, layout: REAL_LAYOUT })
    expect(result).toEqual({
      status: 'fallback',
      layout: DEFAULT_AGENT_WORKSPACE_LAYOUT,
      reason: 'unsupported-schema-version',
    })
  })

  it('drops leftOrder when the dock edge is not left', () => {
    const result = migrateLayout({
      schemaVersion: 1,
      layout: {
        ...REAL_LAYOUT,
        dock: { kind: 'source', edge: 'right', leftOrder: 'above-chats', sizePx: 400 },
      },
    })
    expect(result.status).toBe('ok')
    if (result.status === 'ok') {
      expect(result.layout.dock).toEqual({ kind: 'source', edge: 'right', sizePx: 400 })
    }
  })
})

describe('clampDockSizePx', () => {
  it('clamps below the minimum up to MIN_DOCK_SIZE_PX', () => {
    expect(clampDockSizePx(10)).toBe(MIN_DOCK_SIZE_PX)
  })

  it('clamps above the maximum down to MAX_DOCK_SIZE_PX', () => {
    expect(clampDockSizePx(99999)).toBe(MAX_DOCK_SIZE_PX)
  })

  it('leaves an in-range size unchanged (after rounding)', () => {
    expect(clampDockSizePx(400)).toBe(400)
  })

  it('falls back to the default for non-finite input', () => {
    expect(clampDockSizePx(Number.NaN)).toBeGreaterThanOrEqual(MIN_DOCK_SIZE_PX)
    expect(clampDockSizePx(Number.POSITIVE_INFINITY)).toBeGreaterThanOrEqual(MIN_DOCK_SIZE_PX)
  })

  it('clamps to the current window/monitor bound when narrower than the max', () => {
    // A restored 900px dock on a 500px-wide window/monitor must not claim it.
    const clamped = clampDockSizePx(900, 500)
    expect(clamped).toBeLessThan(500)
    expect(clamped).toBeGreaterThanOrEqual(MIN_DOCK_SIZE_PX)
  })

  it('never clamps below MIN_DOCK_SIZE_PX even on a tiny window bound', () => {
    expect(clampDockSizePx(900, 50)).toBe(MIN_DOCK_SIZE_PX)
  })
})

describe('defaultConversationPaneState', () => {
  it('starts scrolled to the bottom with no scroll offset, regardless of session', () => {
    expect(defaultConversationPaneState('primary', 'session-a')).toEqual({
      pane: 'primary',
      sessionId: 'session-a',
      scrollTop: 0,
      atBottom: true,
    })
  })

  it('accepts a null session for an empty pane', () => {
    expect(defaultConversationPaneState('secondary', null).sessionId).toBeNull()
  })
})

describe('toPersistedLayout', () => {
  it('wraps the layout with the current schema version, not inside it', () => {
    const persisted = toPersistedLayout(REAL_LAYOUT)
    expect(persisted.schemaVersion).toBe(CURRENT_LAYOUT_SCHEMA_VERSION)
    expect(persisted.layout).toBe(REAL_LAYOUT)
    expect('schemaVersion' in persisted.layout).toBe(false)
  })
})
