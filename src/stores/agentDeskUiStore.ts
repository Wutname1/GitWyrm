import { create } from 'zustand'
import { log, describeError } from '@/lib/log'
import type { SidebarGroupMode } from '@/lib/agentSessionGrouping'
import { resolveSplitCollapse } from '@/lib/agentDeskPaneTargeting'
import {
  DEFAULT_AGENT_WORKSPACE_LAYOUT,
  DEFAULT_DOCK_SIZE_PX,
  clampDockSizePx,
  migrateLayout,
  toPersistedLayout,
  type AgentWorkspaceLayout,
  type DockEdge,
  type DockKind,
  type LeftDockOrder,
  type PaneId,
} from '@/lib/agentWorkspaceLayout'

/**
 * Agent Desk workspace layout and session-scoped composer drafts.
 *
 * Per `docs/agent-desk/architecture.md` section 5, this is deliberately its
 * own store rather than more fields on `workspaceStore`: the layout here is a
 * *view* over sessions (which pane shows which session ID, where the dock
 * sits), not session content, and it must stay swappable/resettable without
 * touching anything `workspaceStore` persists through `settings.json`.
 *
 * Persistence target: `localStorage`, not the Rust `Settings` struct. Two
 * reasons this is the right layer for "harmless UI state" instead of another
 * `toSettings` field:
 *   - Session data (including a future per-session layout hint, if one is
 *     ever needed) already has a durable home under `agent-desk/v1/` on disk
 *     (architecture.md section 2). Layout is explicitly *not* that -- design.md
 *     calls it out as UI state that must never mix into durable conversation
 *     content, and `settings.json` is main-window/whole-app preference state,
 *     not a second place for it to leak into.
 *   - `src/lib/avatarSource.ts` already establishes the pattern this project
 *     uses for exactly this tier of state: versioned `localStorage` key,
 *     try/catch around parse, safe in-memory fallback on any failure, debounced
 *     writes. Losing this file costs nothing worse than Agent Desk opening
 *     with its default single-pane layout -- which is also what happens on a
 *     first run -- so it does not need atomic-write-and-rename durability or a
 *     Tauri round-trip.
 *
 * Versioning and fallback go through `src/lib/agentWorkspaceLayout.ts`
 * (`migrateLayout`), which mirrors the Rust session schema-version shape in
 * `src-tauri/src/agentdesk/model.rs`. See that file's header comment for the
 * full reasoning.
 *
 * Composer drafts are kept in memory only, never persisted -- following
 * `src/stores/specDraftStore.ts`'s explicit reasoning for spec-file edits:
 * "anything stronger would mean an invisible copy of a file competing with
 * what git and other tools see." The same logic applies here, arguably more
 * strongly: a spec draft is at least a working copy of a file that exists.
 * An unsent Agent Desk message has no counterpart anywhere else. Writing it
 * to disk would mean a user's half-typed, unreviewed prompt to an AI agent
 * silently outlives the window and survives a "close without sending" that
 * looks -- and is meant to look -- like it discarded nothing durable. The
 * spec's own requirement ("Unsent drafts belong to sessions") only promises
 * the draft survives *pane replacement within the running app*, not restart;
 * `design.md`'s persisted-layout list does not include drafts either. Keeping
 * them in memory satisfies the spec exactly, with a smaller privacy footprint
 * than persisting would have.
 */

const STORAGE_KEY = 'gitwyrm.agentDeskWorkspaceLayout.v1'

interface ComposerDraft {
  text: string
}

/** What fills the centre of the Agent Desk window: the chat panes, or one of the whole-window takeovers. */
export type AgentDeskCenterView = 'conversation' | 'openspec' | 'setup' | 'import'

interface AgentDeskUiState {
  /** False until `hydrate()` has run once; guards persistence like `workspaceStore.hydrated`. */
  hydrated: boolean
  layout: AgentWorkspaceLayout
  /** Session ID -> unsent composer text. Absent key means no draft. */
  drafts: Record<string, ComposerDraft | undefined>
  /**
   * Centre takeover, in the store rather than `AgentDeskView` local state so
   * a message link deep inside a pane can switch to the Spec view without a
   * callback threaded through every pane component. Never persisted: a
   * window always reopens on the conversation.
   */
  centerView: AgentDeskCenterView
  /**
   * Session ID -> execution highlighted in that chat's Agent graph panel.
   * Lives here (not in `AgentGraphPanel` state) so a "View in graph" link
   * can pick a node before the panel has even been opened. Never persisted.
   */
  graphSelection: Record<string, string | undefined>

  /** Read persisted layout (if any) and mark the store ready to persist further changes. */
  hydrate: (windowBoundPx?: number) => void

  // --- Pane / session selection --------------------------------------------------
  /** Replace the session shown in the given pane. Does not change `activePane`. */
  setPaneSession: (pane: PaneId, sessionId: string | null) => void
  setActivePane: (pane: PaneId) => void
  /** Fall back a pane whose session no longer exists to the newest valid session. */
  restorePaneToFallback: (pane: PaneId, fallbackSessionId: string | null) => void

  // --- Split view -----------------------------------------------------------------
  openSplit: () => void
  /** Collapse Split View, keeping the active pane's session as the sole one (spec 5.6). */
  closeSplit: () => void

  // --- Source bars / dock -----------------------------------------------------------
  setSourceBarsVisible: (visible: boolean) => void
  /** How the chat list is grouped. Persisted: see the field's own doc. */
  setSidebarGrouping: (mode: SidebarGroupMode) => void
  toggleSourceBarsVisible: () => void
  openDock: (kind: DockKind, edge: DockEdge, leftOrder?: LeftDockOrder) => void
  moveDock: (edge: DockEdge, leftOrder?: LeftDockOrder) => void
  resizeDock: (sizePx: number, windowBoundPx?: number) => void
  closeDock: () => void

  resetLayout: () => void

  // --- Composer drafts (session-scoped, in-memory only) ----------------------------
  setDraft: (sessionId: string, text: string) => void
  getDraft: (sessionId: string) => string
  /** Called once a session's Send is accepted -- clears only that session's draft. */
  clearDraft: (sessionId: string) => void

  // --- Centre view and graph selection (in-memory only) ------------------------------
  setCenterView: (view: AgentDeskCenterView) => void
  /** Whether the usage card is rolled up. One choice for the window, not
      per chat: it is a preference about how much detail to show, and having
      it reset on every chat switch made it feel broken. */
  usageCollapsed: boolean
  setUsageCollapsed: (collapsed: boolean) => void
  /** Highlight one execution in a chat's graph panel; null falls back to the panel's default (the lead). */
  selectGraphNode: (sessionId: string, executionId: string | null) => void
}

let persistTimer: ReturnType<typeof setTimeout> | null = null

/** Debounced write, mirroring `workspaceStore`'s `schedulePersist` shape. */
function schedulePersist(layout: AgentWorkspaceLayout, hydrated: boolean) {
  if (!hydrated) return
  if (persistTimer) clearTimeout(persistTimer)
  persistTimer = setTimeout(() => {
    persistTimer = null
    try {
      localStorage.setItem(STORAGE_KEY, JSON.stringify(toPersistedLayout(layout)))
    } catch (e) {
      // Quota or private-mode failures leave the in-memory layout intact;
      // this is UI-preference state, so a dropped write is not worth surfacing.
      log.warn(`agentDeskUiStore: failed to persist layout: ${describeError(e)}`)
    }
  }, 300)
}

export const useAgentDeskUiStore = create<AgentDeskUiState>((set, get) => ({
  hydrated: false,
  layout: DEFAULT_AGENT_WORKSPACE_LAYOUT,
  drafts: {},
  centerView: 'conversation',
  graphSelection: {},

  hydrate: (windowBoundPx) => {
    let layout = DEFAULT_AGENT_WORKSPACE_LAYOUT
    try {
      const raw = localStorage.getItem(STORAGE_KEY)
      if (raw) {
        const outcome = migrateLayout(JSON.parse(raw), windowBoundPx)
        layout = outcome.layout
        if (outcome.status === 'fallback') {
          log.warn(`agentDeskUiStore: layout restore fell back to defaults (${outcome.reason})`)
        }
      }
    } catch (e) {
      log.warn(`agentDeskUiStore: failed to read persisted layout: ${describeError(e)}`)
    }
    set({ layout, hydrated: true })
  },

  setPaneSession: (pane, sessionId) => {
    set((s) => {
      // One chat cannot occupy both panes. It would render two transcripts and
      // two composers over the same session, and drafts are keyed by session
      // id -- so typing in one pane would live-overwrite the other. Opening a
      // chat that is already in the other pane swaps the two rather than
      // duplicating it, which is also what a person means by the gesture.
      const other = pane === 'primary' ? s.layout.secondarySessionId : s.layout.primarySessionId
      const displaced = sessionId !== null && other === sessionId
      const mine = pane === 'primary' ? s.layout.primarySessionId : s.layout.secondarySessionId
      const layout: AgentWorkspaceLayout =
        pane === 'primary'
          ? { ...s.layout, primarySessionId: sessionId, secondarySessionId: displaced ? mine : s.layout.secondarySessionId }
          : { ...s.layout, secondarySessionId: sessionId, primarySessionId: displaced ? mine : s.layout.primarySessionId }
      schedulePersist(layout, s.hydrated)
      return { layout }
    })
  },

  setActivePane: (pane) => {
    set((s) => {
      const layout = { ...s.layout, activePane: pane }
      schedulePersist(layout, s.hydrated)
      return { layout }
    })
  },

  restorePaneToFallback: (pane, fallbackSessionId) => {
    get().setPaneSession(pane, fallbackSessionId)
  },

  openSplit: () => {
    set((s) => {
      const layout = { ...s.layout, split: true }
      schedulePersist(layout, s.hydrated)
      return { layout }
    })
  },

  closeSplit: () => {
    set((s) => {
      // Spec 5.6: collapsing keeps the *active* pane's session, including
      // promotion of the right pane -- so when secondary was active, its
      // session becomes the sole (primary) one rather than being discarded.
      //
      // That rule lived here as a hand-inlined copy while `resolveSplitCollapse`
      // sat exported, documented and tested sixty lines away with no caller.
      // They agreed, but the last time this codebase kept parallel copies of a
      // rule one of them silently omitted a state, so there is now one.
      const layout: AgentWorkspaceLayout = { ...s.layout, split: false, ...resolveSplitCollapse(s.layout) }
      schedulePersist(layout, s.hydrated)
      return { layout }
    })
  },

  setSourceBarsVisible: (visible) => {
    set((s) => {
      const layout = { ...s.layout, sourceBarsVisible: visible }
      schedulePersist(layout, s.hydrated)
      return { layout }
    })
  },

  toggleSourceBarsVisible: () => {
    get().setSourceBarsVisible(!get().layout.sourceBarsVisible)
  },

  setSidebarGrouping: (mode) => {
    set((s) => {
      const layout = { ...s.layout, sidebarGrouping: mode }
      schedulePersist(layout, s.hydrated)
      return { layout }
    })
  },

  openDock: (kind, edge, leftOrder) => {
    set((s) => {
      const sizePx = s.layout.dock?.sizePx ?? DEFAULT_DOCK_SIZE_PX
      const layout: AgentWorkspaceLayout = {
        ...s.layout,
        dock: { kind, edge, sizePx, ...(edge === 'left' && leftOrder ? { leftOrder } : {}) },
      }
      schedulePersist(layout, s.hydrated)
      return { layout }
    })
  },

  moveDock: (edge, leftOrder) => {
    set((s) => {
      if (!s.layout.dock) return s
      const layout: AgentWorkspaceLayout = {
        ...s.layout,
        dock: { ...s.layout.dock, edge, ...(edge === 'left' && leftOrder ? { leftOrder } : { leftOrder: undefined }) },
      }
      schedulePersist(layout, s.hydrated)
      return { layout }
    })
  },

  resizeDock: (sizePx, windowBoundPx) => {
    set((s) => {
      if (!s.layout.dock) return s
      const layout: AgentWorkspaceLayout = {
        ...s.layout,
        dock: { ...s.layout.dock, sizePx: clampDockSizePx(sizePx, windowBoundPx) },
      }
      schedulePersist(layout, s.hydrated)
      return { layout }
    })
  },

  closeDock: () => {
    set((s) => {
      const layout = { ...s.layout, dock: null }
      schedulePersist(layout, s.hydrated)
      return { layout }
    })
  },

  resetLayout: () => {
    set((s) => {
      schedulePersist(DEFAULT_AGENT_WORKSPACE_LAYOUT, s.hydrated)
      return { layout: DEFAULT_AGENT_WORKSPACE_LAYOUT }
    })
  },

  // Drafts are intentionally left out of `schedulePersist` -- see the file
  // header comment. Nothing here ever calls `localStorage`.
  setDraft: (sessionId, text) => {
    set((s) => ({ drafts: { ...s.drafts, [sessionId]: { text } } }))
  },

  getDraft: (sessionId) => get().drafts[sessionId]?.text ?? '',

  clearDraft: (sessionId) => {
    set((s) => {
      if (!(sessionId in s.drafts)) return s
      const next = { ...s.drafts }
      delete next[sessionId]
      return { drafts: next }
    })
  },

  setCenterView: (view) => set({ centerView: view }),
  usageCollapsed: false,
  setUsageCollapsed: (usageCollapsed) => set({ usageCollapsed }),

  selectGraphNode: (sessionId, executionId) => {
    set((s) => {
      if ((s.graphSelection[sessionId] ?? null) === executionId) return s
      const next = { ...s.graphSelection }
      if (executionId === null) delete next[sessionId]
      else next[sessionId] = executionId
      return { graphSelection: next }
    })
  },
}))

/** Flush any pending debounced write immediately -- call on window close, mirroring `flushPendingSettings`. */
export function flushPendingAgentDeskLayout(): void {
  if (!persistTimer) return
  clearTimeout(persistTimer)
  persistTimer = null
  const s = useAgentDeskUiStore.getState()
  if (!s.hydrated) return
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(toPersistedLayout(s.layout)))
  } catch (e) {
    log.warn(`agentDeskUiStore: failed to flush layout on close: ${describeError(e)}`)
  }
}
