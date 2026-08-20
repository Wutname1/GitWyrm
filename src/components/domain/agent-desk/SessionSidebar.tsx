import { useEffect, useRef, useState } from 'react'
import { PanelLeftClose, PanelLeftOpen } from 'lucide-react'
import type { SessionListFilterInput } from '@/lib/bindings'
import { useAgentSessionHeaders } from '@/hooks/useAgentSessions'
import { useAgentSessionMutations } from '@/hooks/useAgentSessionMutations'
import { NewSessionButton } from '@/components/domain/agent-desk/NewSessionButton'
import { SessionGroups } from '@/components/domain/agent-desk/SessionGroups'
import { cn } from '@/lib/utils'

/** Below this width the sidebar becomes a drawer instead of a fixed column (design.md's "narrow widths" clause, tasks.md 3.1). */
const DRAWER_BREAKPOINT_PX = 620
/** design.md: "Left column: 218-282 px". */
const SIDEBAR_MIN_PX = 218
const SIDEBAR_MAX_PX = 282

interface SessionSidebarProps {
  /** Repo to scope the list to, or null to show every repo's sessions. */
  repoId: string | null
  selectedId: string | null
  /**
   * Selection is reported upward, never applied locally -- per the seam
   * contract, the workspace-layout package (not this sidebar) decides which
   * pane receives it. This component only ever calls this prop; it never
   * writes to a "current session" store of its own.
   */
  onSelectSession: (sessionId: string) => void
  /** Also reported upward; the workspace-layout package owns session creation (architecture.md section 8's kickoff pipeline). */
  onNewSession: () => void
  /** Extra filter beyond repoId, e.g. archived visibility. Defaults to active, non-archived sessions. */
  filter?: Partial<SessionListFilterInput>
}

/**
 * The Agent Desk left column (tasks 3.1-3.7): virtualized one-line session
 * rows with Recent/Project/Diff grouping, New chat, rename/archive, and a
 * narrow-width drawer.
 *
 * Ownership boundary: this component and everything it renders decide *how*
 * sessions are listed and grouped. It never decides *what happens* when one
 * is picked or created -- both go out through injected callbacks so the
 * workspace-layout package (a different cluster) can route them to whichever
 * pane is active, per the shell-scaffold brief's seam contract.
 */
export function SessionSidebar({
  repoId,
  selectedId,
  onSelectSession,
  onNewSession,
  filter,
}: SessionSidebarProps) {
  const { headers, isLoading } = useAgentSessionHeaders({
    repoId: repoId ?? null,
    projectPath: null,
    states: [],
    sourceKinds: [],
    hasChangedFiles: null,
    archived: false,
    titleContains: null,
    ...filter,
  })
  const { rename, archive, markRead } = useAgentSessionMutations()

  // Narrow-width drawer (task: "a hidden sidebar with no reopen affordance
  // violates house Rule #1"). Tracks the *container's* width via
  // ResizeObserver -- matching the pattern RepositoryTabs.tsx already uses
  // for its own responsive behavior -- rather than a window-level media
  // query, since the sidebar can be narrow inside a wide window (e.g. Split
  // View with a docked right panel) and vice versa.
  const containerRef = useRef<HTMLDivElement>(null)
  const [containerWidth, setContainerWidth] = useState<number | null>(null)
  const [drawerOpen, setDrawerOpen] = useState(false)

  useEffect(() => {
    const node = containerRef.current
    if (!node) return
    const observer = new ResizeObserver((entries) => {
      const entry = entries[0]
      if (entry) setContainerWidth(entry.contentRect.width)
    })
    observer.observe(node)
    return () => observer.disconnect()
  }, [])

  const isDrawerMode = containerWidth != null && containerWidth < DRAWER_BREAKPOINT_PX

  // Selecting a session while the sidebar is a drawer implies the user is
  // done with it -- closing automatically keeps the drawer from covering the
  // conversation it just opened (visible response to the click, per house
  // Rule #1, without a second explicit close step).
  const handleSelect = (sessionId: string) => {
    onSelectSession(sessionId)
    if (isDrawerMode) setDrawerOpen(false)
    const target = headers.find((h) => h.sessionId === sessionId)
    if (target?.unread) markRead.mutate(sessionId)
  }

  const handleNewSession = () => {
    onNewSession()
    if (isDrawerMode) setDrawerOpen(false)
  }

  const body = (
    <div
      className="flex h-full flex-col bg-panel"
      style={!isDrawerMode ? { minWidth: SIDEBAR_MIN_PX, maxWidth: SIDEBAR_MAX_PX } : undefined}
    >
      <div className="flex-none p-1.5 pb-1">
        <NewSessionButton onNewSession={handleNewSession} />
      </div>
      <SessionGroups
        headers={headers}
        selectedId={selectedId}
        onSelectSession={handleSelect}
        onRename={(sessionId, title) => rename.mutate({ sessionId, title })}
        onArchive={(sessionId, archived) => archive.mutate({ sessionId, archived })}
      />
      {isLoading && headers.length === 0 && (
        <p className="flex-none px-3 py-2 text-2xs text-muted-foreground">Loading chats…</p>
      )}
    </div>
  )

  if (!isDrawerMode) {
    return (
      <div ref={containerRef} className="flex h-full min-w-0 border-r border-border">
        {body}
      </div>
    )
  }

  // Drawer mode: the column collapses to a thin strip with a reopen toggle
  // (the mockup just does `display:none` at <=620px with no way back --
  // that's the gap this task brief calls out). The toggle is always visible
  // so the affordance survives even when the drawer is closed.
  return (
    <div ref={containerRef} className="relative flex h-full min-w-0">
      <div className="flex-none border-r border-border bg-panel">
        <button
          type="button"
          onClick={() => setDrawerOpen((v) => !v)}
          aria-expanded={drawerOpen}
          aria-label={drawerOpen ? 'Close chat list' : 'Open chat list'}
          className="flex h-10 w-9 items-center justify-center text-muted-foreground hover:bg-panel2 hover:text-foreground"
        >
          {drawerOpen ? <PanelLeftClose size={15} /> : <PanelLeftOpen size={15} />}
        </button>
      </div>
      {drawerOpen && (
        <>
          {/* Click-outside backdrop; keyboard users close via the toggle or Escape. */}
          <button
            type="button"
            aria-label="Close chat list"
            onClick={() => setDrawerOpen(false)}
            className="fixed inset-0 z-40 cursor-default bg-black/40"
          />
          <div
            className={cn('absolute inset-y-0 left-9 z-50 shadow-lg')}
            style={{ width: SIDEBAR_MIN_PX }}
            onKeyDown={(e) => {
              if (e.key === 'Escape') setDrawerOpen(false)
            }}
          >
            {body}
          </div>
        </>
      )}
    </div>
  )
}
