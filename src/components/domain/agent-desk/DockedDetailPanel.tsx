import { Coins, GitFork, Gauge, Link2, MoreHorizontal, PanelBottom, PanelLeft, PanelRight, X } from 'lucide-react'
import type { AgentSession } from '@/lib/bindings'
import { ResizeHandle } from '@/components/ui/ResizeHandle'
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu'
import { DEFAULT_DOCK_SIZE_PX, MAX_DOCK_SIZE_PX, MIN_DOCK_SIZE_PX, type DockEdge, type DockKind, type LeftDockOrder } from '@/lib/agentWorkspaceLayout'
import { ALL_DOCK_ZONES, type DockZone } from '@/lib/agentDeskDockPlacement'
import { sessionHasGraph } from '@/lib/agentDeskGraph'
import { useAgentDeskPanelDrag } from '@/hooks/useAgentDeskPanelDrag'
import { SessionSourcePanel } from './SessionSourcePanel'
import { SessionContextPanel } from './SessionContextPanel'
import { SessionUsageCard } from './SessionUsageCard'
import { AgentGraphPanel } from './AgentGraphPanel'

const DETAIL_META: Record<DockKind, { label: string; icon: React.ComponentType<{ size?: number; className?: string; 'aria-hidden'?: boolean }> }> = {
  source: { label: 'Source', icon: Link2 },
  context: { label: 'Context', icon: Gauge },
  usage: { label: 'Usage', icon: Coins },
  graph: { label: 'Agent graph', icon: GitFork },
}

const ZONE_LABEL: Record<DockZone, string> = {
  right: 'Right',
  bottom: 'Bottom',
  'left-above': 'Left, above chats',
  'left-below': 'Left, below chats',
}

const ZONE_ICON: Record<DockZone, React.ComponentType<{ size?: number; className?: string; 'aria-hidden'?: boolean }>> = {
  right: PanelRight,
  bottom: PanelBottom,
  'left-above': PanelLeft,
  'left-below': PanelLeft,
}

/**
 * The one shared dock host (tasks.md 7.2): follows the active pane's
 * session, and can sit right / bottom / left-above-chats / left-below-chats
 * (7.3). Every placement is reachable three ways -- pointer drag onto this
 * component's own header (re-docking an already-pinned panel), the Move
 * menu, and keyboard commands -- all iterating the same `ALL_DOCK_ZONES`
 * list so no path can reach a placement another path cannot (7.4/7.5/7.7).
 *
 * Resize uses the shared `ResizeHandle` (7.8): axis follows the edge (a
 * right dock resizes its width, a bottom dock its height), and the min/max
 * come from `agentWorkspaceLayout.ts` so the same clamp `hydrate()` applies
 * on restore is what a live drag can reach too.
 *
 * Content refresh (7.9): this component receives `session` from whichever
 * pane is active, the same way `PaneDetailPopover` does -- switching panes
 * or switching a pane's session re-renders this with a new `session`, and
 * each detail component (`SessionSourcePanel`/`SessionContextPanel`/
 * `AgentGraphPanel`) is itself pure over that prop, so content refreshes
 * automatically with no separate "dock refetch" path to keep in sync.
 */
export function DockedDetailPanel({
  kind,
  edge,
  leftOrder,
  sizePx,
  session,
  onOpenSource,
  onMove,
  onResize,
  onResizeReset,
  onUnpin,
}: {
  kind: DockKind
  edge: DockEdge
  leftOrder?: LeftDockOrder
  sizePx: number
  session: AgentSession | null
  onOpenSource?: () => void
  onMove: (zone: DockZone) => void
  onResize: (sizePx: number) => void
  onResizeReset: () => void
  onUnpin: () => void
}) {
  const { dragSourceProps, hoverZone } = useAgentDeskPanelDrag(kind, edgeToZone(edge, leftOrder))
  const { label, icon: Icon } = DETAIL_META[kind]
  const axis: 'x' | 'y' = edge === 'bottom' ? 'y' : 'x'
  // A left/right dock's handle grows the panel when dragged toward the
  // center of the window; for a right dock that is leftward (direction -1),
  // for a left dock that is rightward (direction 1). A bottom dock grows
  // when dragged upward (direction -1).
  const direction: 1 | -1 = edge === 'right' ? -1 : edge === 'bottom' ? -1 : 1

  return (
    <div
      className="relative flex min-h-0 flex-none flex-col border-border bg-panel"
      style={axis === 'x' ? { width: sizePx } : { height: sizePx }}
      data-dock-edge={edge}
    >
      <div
        {...dragSourceProps()}
        className="flex h-8 flex-none cursor-grab items-center gap-1.5 border-b border-border px-2 active:cursor-grabbing"
      >
        <Icon size={13} className="flex-none text-muted-foreground" aria-hidden />
        <strong className="text-2xs font-semibold text-foreground">{label}</strong>
        <span className="ml-1 truncate text-2xs text-muted-foreground">follows active chat</span>

        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <button
              type="button"
              aria-label="Move panel"
              className="ml-auto flex h-6 w-6 flex-none items-center justify-center rounded text-muted-foreground hover:bg-panel3 hover:text-foreground"
            >
              <MoreHorizontal size={13} aria-hidden />
            </button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="end">
            {ALL_DOCK_ZONES.map((zone) => {
              const ZoneIcon = ZONE_ICON[zone]
              return (
                <DropdownMenuItem key={zone} onSelect={() => onMove(zone)}>
                  <ZoneIcon size={13} aria-hidden />
                  Move to {ZONE_LABEL[zone]}
                </DropdownMenuItem>
              )
            })}
          </DropdownMenuContent>
        </DropdownMenu>

        <button
          type="button"
          aria-label="Unpin panel"
          onClick={onUnpin}
          className="flex h-6 w-6 flex-none items-center justify-center rounded text-muted-foreground hover:bg-panel3 hover:text-foreground"
        >
          <X size={13} aria-hidden />
        </button>
      </div>

      <div className="min-h-0 flex-1 overflow-y-auto">
        {session == null ? (
          <p className="p-3 text-2xs leading-relaxed text-muted-foreground">Pick a chat to see its details.</p>
        ) : kind === 'source' ? (
          <SessionSourcePanel session={session} onOpenSource={onOpenSource} />
        ) : kind === 'context' ? (
          <SessionContextPanel session={session} />
        ) : kind === 'usage' ? (
          <div className="p-2">
            <SessionUsageCard sessionId={session.header.sessionId} />
          </div>
        ) : sessionHasGraph(session) ? (
          <AgentGraphPanel session={session} />
        ) : (
          // The dock follows the active chat, so a pinned graph panel can
          // land on a chat that has no graph. Say so instead of showing an
          // empty diagram.
          <p className="p-3 text-2xs leading-relaxed text-muted-foreground">
            This chat is one agent working alone, so there is no team to show. Pick a chat with helpers, or unpin this
            panel.
          </p>
        )}
      </div>

      <ResizeHandle
        ariaLabel={`Resize the ${label.toLowerCase()} panel`}
        value={sizePx}
        min={MIN_DOCK_SIZE_PX}
        max={MAX_DOCK_SIZE_PX}
        defaultValue={DEFAULT_DOCK_SIZE_PX}
        direction={direction}
        axis={axis}
        onChange={onResize}
        onReset={onResizeReset}
        className={
          edge === 'right'
            ? 'left-0 -translate-x-1/2'
            : edge === 'bottom'
              ? 'top-0 -translate-y-1/2'
              : 'right-0 translate-x-1/2'
        }
      />

      {/* Highlight while some panel (possibly this one) is dragging over any
          zone -- the real drop targets live in the shell around the pane
          grid (`AgentDeskDockDropZones`), including one for this dock's own
          current zone so dropping a *different* panel here swaps it in. */}
      {hoverZone === edgeToZone(edge, leftOrder) && (
        <div className="pointer-events-none absolute inset-0 rounded-sm ring-2 ring-inset ring-primary/60" aria-hidden />
      )}
    </div>
  )
}

function edgeToZone(edge: DockEdge, leftOrder?: LeftDockOrder): DockZone {
  if (edge === 'right') return 'right'
  if (edge === 'bottom') return 'bottom'
  return leftOrder === 'below-chats' ? 'left-below' : 'left-above'
}
