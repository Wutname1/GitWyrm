/**
 * The Agent Desk workspace layout: which panes are open, which sessions they
 * show, and where the detail dock sits. This is "harmless UI state" per
 * `design.md` -- it is never mixed into durable session content, and losing
 * it costs nothing worse than the window looking like a fresh install.
 *
 * Versioning mirrors `src-tauri/src/agentdesk/model.rs`
 * (`CURRENT_SCHEMA_VERSION` + `migrate_session`/`migrate_header`): a plain
 * numeric `schemaVersion`, one `migrateLayout` entry point that inspects it
 * before touching any other field, and an explicit reject for a version
 * newer than this build understands rather than guessing at fields that do
 * not exist yet. The Rust side treats "too new" and "unreadable" as
 * different failures (`UnsupportedSchemaVersion` vs `Malformed`) so callers
 * do not confuse a future build's file with a corrupt one; `migrateLayout`
 * keeps that same distinction in its result `reason`, even though both
 * outcomes fall back to the same safe default here -- there is no session
 * list to skip-and-continue for, just one layout object, so the fallback is
 * always "use defaults" rather than "drop this one and keep the rest."
 *
 * There is no schema version 0 and no predecessor to migrate from yet, so
 * `migrateLayout` only validates today. It is still a real function with a
 * real seam (the `schemaVersion === 1` branch) so the first breaking change
 * has one place to extend instead of a decision about where migration code
 * should even live -- exactly the reasoning left on `migrate_session`.
 */

export const CURRENT_LAYOUT_SCHEMA_VERSION = 1

export type PaneId = 'primary' | 'secondary'
export type DockKind = 'source' | 'context' | 'graph'
export type DockEdge = 'left' | 'right' | 'bottom'
export type LeftDockOrder = 'above-chats' | 'below-chats'

export interface DockState {
  kind: DockKind
  edge: DockEdge
  /** Only meaningful when `edge === 'left'`; absent elsewhere. */
  leftOrder?: LeftDockOrder
  sizePx: number
}

/**
 * Matches `AgentWorkspaceLayout` in `design.md` / `docs/agent-desk/architecture.md`
 * section 5, with one addition: `schemaVersion`. Neither source shows the
 * field on the interface itself, but both are written before task 2's
 * versioning requirement was scoped out in detail. The version wraps the
 * payload (`{ schemaVersion, layout }`, see `PersistedAgentWorkspaceLayout`
 * below) rather than being spliced into `AgentWorkspaceLayout` itself, so the
 * shape callers pass around at runtime stays exactly what those documents
 * describe -- `migrateLayout` is the only place that ever sees the version
 * number.
 */
export interface AgentWorkspaceLayout {
  split: boolean
  activePane: PaneId
  primarySessionId: string | null
  secondarySessionId: string | null
  sourceBarsVisible: boolean
  dock: DockState | null
}

/** The on-disk/on-storage envelope: version alongside, not inside, the layout. */
export interface PersistedAgentWorkspaceLayout {
  schemaVersion: number
  layout: AgentWorkspaceLayout
}

/**
 * What one pane holds at runtime: a selected session ID plus its own
 * transient viewport state. Per `design.md`'s "Panes select views of
 * sessions" -- a pane never owns or copies the conversation, so this does
 * NOT include transcript content, only the pointer to it and view-only state
 * that resets to a sane default when the pane's session changes.
 *
 * Deliberately not part of `AgentWorkspaceLayout` and not persisted:
 * `scrollTop`/`atBottom` are exactly the "transcript scroll" the spec's
 * "Workspace layout survives restart" requirement calls out as something
 * that must NOT be restored (open popovers, drag state, and hover state are
 * the other named examples). `sessionId` duplicates
 * `primary/secondarySessionId` from the layout on purpose: the layout is the
 * persisted source of truth for *which* session a pane shows, while this
 * type is the shape a `ConversationPane` component reads/writes locally
 * while it is mounted, keyed by `PaneId`.
 */
export interface ConversationPaneState {
  pane: PaneId
  sessionId: string | null
  /** Pixel scroll offset within the transcript. Ephemeral -- never persisted. */
  scrollTop: number
  /** Whether the pane is pinned to the newest message. Ephemeral -- never persisted. */
  atBottom: boolean
}

export function defaultConversationPaneState(pane: PaneId, sessionId: string | null): ConversationPaneState {
  return { pane, sessionId, scrollTop: 0, atBottom: true }
}

export const DEFAULT_DOCK_SIZE_PX = 360
export const MIN_DOCK_SIZE_PX = 240
export const MAX_DOCK_SIZE_PX = 960

export const DEFAULT_AGENT_WORKSPACE_LAYOUT: AgentWorkspaceLayout = {
  split: false,
  activePane: 'primary',
  primarySessionId: null,
  secondarySessionId: null,
  sourceBarsVisible: true,
  dock: null,
}

/** Clamp a restored dock size to a safe, always-visible range. */
export function clampDockSizePx(sizePx: number, windowBoundPx?: number): number {
  if (!Number.isFinite(sizePx)) return DEFAULT_DOCK_SIZE_PX
  let max = MAX_DOCK_SIZE_PX
  if (typeof windowBoundPx === 'number' && Number.isFinite(windowBoundPx)) {
    // Leave room for the chat pane itself; never let the dock claim the
    // entire window on a small or heavily scaled display.
    const windowMax = Math.floor(windowBoundPx * 0.7)
    max = Math.min(max, Math.max(MIN_DOCK_SIZE_PX, windowMax))
  }
  return Math.min(max, Math.max(MIN_DOCK_SIZE_PX, Math.round(sizePx)))
}

function isPaneId(v: unknown): v is PaneId {
  return v === 'primary' || v === 'secondary'
}

function isDockKind(v: unknown): v is DockKind {
  return v === 'source' || v === 'context' || v === 'graph'
}

function isDockEdge(v: unknown): v is DockEdge {
  return v === 'left' || v === 'right' || v === 'bottom'
}

function isLeftDockOrder(v: unknown): v is LeftDockOrder {
  return v === 'above-chats' || v === 'below-chats'
}

function isNullableSessionId(v: unknown): v is string | null {
  return v === null || typeof v === 'string'
}

/**
 * Validates a `dock` value shaped like the current schema. Returns `null`
 * (a harmless "no dock" default) if the value is not a real dock, rather
 * than throwing -- an undockable panel is never worth losing the rest of a
 * restored layout over.
 */
function readDock(raw: unknown, windowBoundPx?: number): DockState | null {
  if (raw === null || raw === undefined) return null
  if (typeof raw !== 'object') return null
  const r = raw as Record<string, unknown>
  if (!isDockKind(r.kind) || !isDockEdge(r.edge)) return null
  const sizePx = clampDockSizePx(typeof r.sizePx === 'number' ? r.sizePx : DEFAULT_DOCK_SIZE_PX, windowBoundPx)
  const dock: DockState = { kind: r.kind, edge: r.edge, sizePx }
  if (r.edge === 'left' && isLeftDockOrder(r.leftOrder)) {
    dock.leftOrder = r.leftOrder
  }
  return dock
}

export type LayoutMigrationOutcome =
  | { status: 'ok'; layout: AgentWorkspaceLayout }
  | { status: 'fallback'; layout: AgentWorkspaceLayout; reason: 'malformed' | 'unsupported-schema-version' }

/**
 * Brings a raw parsed value up to `CURRENT_LAYOUT_SCHEMA_VERSION`, or falls
 * back to `DEFAULT_AGENT_WORKSPACE_LAYOUT` and says why. Never throws --
 * this runs during app startup (`design.md`: "Restore layout before first
 * paint"), and a throw there would be worse than an empty workspace.
 *
 * `windowBoundPx`, when given, additionally clamps the restored dock size to
 * the current window/monitor per the spec's "Clamp restored sizes to the
 * current window and attached monitor."
 */
export function migrateLayout(raw: unknown, windowBoundPx?: number): LayoutMigrationOutcome {
  if (raw === null || typeof raw !== 'object') {
    return { status: 'fallback', layout: DEFAULT_AGENT_WORKSPACE_LAYOUT, reason: 'malformed' }
  }
  const r = raw as Record<string, unknown>
  const found = typeof r.schemaVersion === 'number' ? r.schemaVersion : undefined

  if (found === undefined) {
    return { status: 'fallback', layout: DEFAULT_AGENT_WORKSPACE_LAYOUT, reason: 'malformed' }
  }
  if (found > CURRENT_LAYOUT_SCHEMA_VERSION) {
    // A future build wrote fields this one does not understand yet. Reading
    // it partially would misread rather than migrate it, so this falls back
    // exactly like a version this old build has never heard of -- never
    // guess at what a newer schema might mean.
    return { status: 'fallback', layout: DEFAULT_AGENT_WORKSPACE_LAYOUT, reason: 'unsupported-schema-version' }
  }
  if (found !== 1) {
    // No migration path exists yet; v1 is the floor, same as
    // `migrate_session`'s `Some(v) => Err(UnsupportedSchemaVersion)` arm.
    return { status: 'fallback', layout: DEFAULT_AGENT_WORKSPACE_LAYOUT, reason: 'unsupported-schema-version' }
  }

  const payload = r.layout
  if (payload === null || typeof payload !== 'object') {
    return { status: 'fallback', layout: DEFAULT_AGENT_WORKSPACE_LAYOUT, reason: 'malformed' }
  }
  const p = payload as Record<string, unknown>

  const split = typeof p.split === 'boolean' ? p.split : DEFAULT_AGENT_WORKSPACE_LAYOUT.split
  const activePane = isPaneId(p.activePane) ? p.activePane : DEFAULT_AGENT_WORKSPACE_LAYOUT.activePane
  const primarySessionId = isNullableSessionId(p.primarySessionId)
    ? p.primarySessionId
    : DEFAULT_AGENT_WORKSPACE_LAYOUT.primarySessionId
  const secondarySessionId = isNullableSessionId(p.secondarySessionId)
    ? p.secondarySessionId
    : DEFAULT_AGENT_WORKSPACE_LAYOUT.secondarySessionId
  const sourceBarsVisible =
    typeof p.sourceBarsVisible === 'boolean' ? p.sourceBarsVisible : DEFAULT_AGENT_WORKSPACE_LAYOUT.sourceBarsVisible
  const dock = readDock(p.dock, windowBoundPx)

  return {
    status: 'ok',
    layout: { split, activePane, primarySessionId, secondarySessionId, sourceBarsVisible, dock },
  }
}

/** Wraps a layout for persistence at the current schema version. */
export function toPersistedLayout(layout: AgentWorkspaceLayout): PersistedAgentWorkspaceLayout {
  return { schemaVersion: CURRENT_LAYOUT_SCHEMA_VERSION, layout }
}
