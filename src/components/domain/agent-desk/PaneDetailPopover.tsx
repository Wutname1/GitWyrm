import { useRef, useState } from 'react'
import { GitFork, Link2, Gauge, PanelBottom, PanelLeft, PanelRight, X } from 'lucide-react'
import type { AgentSession } from '@/lib/bindings'
import { Popover, PopoverAnchor, PopoverContent } from '@/components/ui/popover'
import { cn } from '@/lib/utils'
import type { DockKind } from '@/lib/agentWorkspaceLayout'
import { SessionSourcePanel } from './SessionSourcePanel'
import { SessionContextPanel } from './SessionContextPanel'
import { AgentGraphPanel } from './AgentGraphPanel'

const DETAIL_META: Record<DockKind, { label: string; icon: React.ComponentType<{ size?: number; className?: string; 'aria-hidden'?: boolean }> }> = {
  source: { label: 'Show source', icon: Link2 },
  context: { label: 'Show context', icon: Gauge },
  graph: { label: 'Show agent graph', icon: GitFork },
}

/**
 * The Source/Context/Graph icon buttons that live in every conversation
 * pane's header (tasks.md 6.1), plus the popover they open (6.3-6.7).
 *
 * Scoped entirely to `session`, not a global "current session" -- passed in
 * by the pane that owns this header, so two panes showing two different
 * sessions never leak each other's Source/Context/Graph state into the
 * wrong popover (6.2/6.8). Re-clicking the button that is already open
 * closes it, matching the mockup's toggle behavior; clicking a different
 * button switches which detail is shown without a second click to close the
 * first.
 *
 * Escape/outside-click close come from Radix `Popover` itself (it already
 * satisfies 6.5), and its `PopoverContent` already runs every open through
 * `overlayCollisionBoundary()`/`OVERLAY_COLLISION_PADDING` (see
 * `src/components/ui/popover.tsx`), so this component does not need to
 * reimplement either.
 */
export function PaneDetailPopover({
  session,
  headerAnchorRef,
  onOpenSource,
  onPin,
}: {
  session: AgentSession | null
  headerAnchorRef?: React.RefObject<HTMLDivElement | null>
  onOpenSource?: () => void
  /** Pin the currently open detail kind to the shared dock at the given edge (tasks.md 6.3's footer row, 7.4). */
  onPin: (kind: DockKind, edge: 'left' | 'right' | 'bottom') => void
}) {
  const [open, setOpen] = useState<DockKind | null>(null)
  // tasks.md 6.4: focus returns to the icon that opened the popover on
  // close, not just "wherever the browser happens to leave it" -- each
  // button's own element is the thing to refocus, so this tracks whichever
  // one was clicked most recently.
  const lastTriggerRef = useRef<HTMLButtonElement | null>(null)

  const closeAndReturnFocus = () => {
    setOpen(null)
    lastTriggerRef.current?.focus()
  }

  return (
    <div className="flex items-center gap-0.5" aria-label="Chat panels">
      {(Object.keys(DETAIL_META) as DockKind[]).map((kind) => {
        const { label, icon: Icon } = DETAIL_META[kind]
        const isOpen = open === kind
        // tasks.md 6.7: a chat with no helpers has no graph to show, so the
        // Graph button says so honestly (dimmed, with a reason on hover)
        // rather than looking live and opening an empty panel. It stays
        // clickable -- the panel itself explains how to get a graph -- but
        // it no longer reads as "there is something here".
        const graphEmpty = kind === 'graph' && (session?.executions.length ?? 0) === 0
        return (
          <button
            key={kind}
            type="button"
            aria-label={graphEmpty ? 'Show agent graph (no helpers yet)' : label}
            aria-pressed={isOpen}
            title={graphEmpty ? 'This chat is running solo, so there is no agent graph yet.' : undefined}
            onClick={(e) => {
              lastTriggerRef.current = e.currentTarget
              setOpen(isOpen ? null : kind)
            }}
            className={cn(
              'flex h-[26px] w-[26px] flex-none items-center justify-center rounded border border-transparent text-muted-foreground hover:border-border hover:bg-panel3 hover:text-foreground',
              graphEmpty && 'opacity-45',
              isOpen && 'border-border bg-panel3 text-foreground'
            )}
          >
            <Icon size={14} aria-hidden />
          </button>
        )
      })}

      {/* One popover, anchored to the pane's own header via `headerAnchorRef`,
          repointed at whichever kind was most recently selected. Kept as a
          single `Popover` (not three) so only one can ever be open per pane,
          matching the mockup's single `.ag-detail-popover`. The three
          buttons above are the real triggers -- each toggles `open` itself
          so re-clicking the same one closes it, which Radix's own
          trigger-toggle does not give us when three buttons share one
          popover -- so this uses a controlled `Popover` with no
          `PopoverTrigger` at all, anchored via `PopoverAnchor`. */}
      <Popover open={open !== null} onOpenChange={(next) => !next && closeAndReturnFocus()}>
        <PopoverAnchor ref={headerAnchorRef} />
        <PopoverContent
          side="bottom"
          align="end"
          sideOffset={6}
          className="w-[min(340px,calc(100vw-2rem))] overflow-hidden p-0"
        >
          {open && (
            <div className="flex flex-col">
              <div className="flex h-8 flex-none items-center gap-1.5 border-b border-border px-2">
                <strong className="text-[10.5px] font-semibold text-foreground">{DETAIL_META[open].label.replace('Show ', '')}</strong>
                <button
                  type="button"
                  aria-label="Close"
                  onClick={closeAndReturnFocus}
                  className="ml-auto flex h-6 w-6 flex-none items-center justify-center rounded text-muted-foreground hover:bg-panel3 hover:text-foreground"
                >
                  <X size={13} aria-hidden />
                </button>
              </div>

              <div className="max-h-80 overflow-y-auto">
                {session == null ? (
                  <p className="p-3 text-2xs leading-relaxed text-muted-foreground">
                    Pick a chat to see its details.
                  </p>
                ) : open === 'source' ? (
                  <SessionSourcePanel session={session} onOpenSource={onOpenSource} />
                ) : open === 'context' ? (
                  <SessionContextPanel session={session} />
                ) : (
                  <AgentGraphPanel session={session} />
                )}
              </div>

              <div className="flex flex-none items-center gap-1 border-t border-border bg-panel2 px-2 py-1.5">
                <span className="mr-auto text-[9.5px] font-semibold uppercase tracking-wide text-muted-foreground">
                  Pin panel
                </span>
                <button
                  type="button"
                  onClick={() => {
                    if (!open) return
                    onPin(open, 'left')
                    closeAndReturnFocus()
                  }}
                  className="flex h-6 items-center gap-1 rounded border border-border px-1.5 text-[9px] font-semibold text-sub hover:border-border-strong hover:text-foreground"
                >
                  <PanelLeft size={12} aria-hidden />
                  Left
                </button>
                <button
                  type="button"
                  onClick={() => {
                    if (!open) return
                    onPin(open, 'bottom')
                    closeAndReturnFocus()
                  }}
                  className="flex h-6 items-center gap-1 rounded border border-border px-1.5 text-[9px] font-semibold text-sub hover:border-border-strong hover:text-foreground"
                >
                  <PanelBottom size={12} aria-hidden />
                  Bottom
                </button>
                <button
                  type="button"
                  onClick={() => {
                    if (!open) return
                    onPin(open, 'right')
                    closeAndReturnFocus()
                  }}
                  className="flex h-6 items-center gap-1 rounded border border-border px-1.5 text-[9px] font-semibold text-sub hover:border-border-strong hover:text-foreground"
                >
                  <PanelRight size={12} aria-hidden />
                  Right
                </button>
              </div>
            </div>
          )}
        </PopoverContent>
      </Popover>
    </div>
  )
}
