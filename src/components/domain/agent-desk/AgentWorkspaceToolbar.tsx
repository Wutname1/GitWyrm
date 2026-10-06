import { Columns2, PanelsTopLeft, RotateCcw } from 'lucide-react'
import {
  DropdownMenu,
  DropdownMenuCheckboxItem,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuSub,
  DropdownMenuSubContent,
  DropdownMenuSubTrigger,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu'
import { cn } from '@/lib/utils'
import { ALL_DOCK_KINDS, ALL_DOCK_ZONES, dockKindLabel, zoneLabel } from '@/lib/agentDeskDock'
import type { DockZone } from '@/lib/agentDeskDockPlacement'
import type { DockKind, DockState } from '@/lib/agentWorkspaceLayout'

/**
 * The layout controls for the chat area: Split View and the Panels menu
 * (pinned panel, source bars, Reset workspace layout).
 *
 * These used to be a full-width "Workspace" strip of their own between the
 * title bar and the chat. They are layout settings most people touch once, so
 * they now sit as two icon buttons in the title bar. Each still shows when it
 * is on, so the current layout is never a mystery.
 */
export function AgentWorkspaceToolbar({
  split,
  onToggleSplit,
  sourceBarsVisible,
  onToggleSourceBars,
  dock,
  onMoveDock,
  onUnpinDock,
  onPinDock,
  onResetLayout,
}: {
  split: boolean
  onToggleSplit: () => void
  sourceBarsVisible: boolean
  onToggleSourceBars: () => void
  dock: DockState | null
  onMoveDock: (zone: DockZone) => void
  onUnpinDock: () => void
  onPinDock: (kind: DockKind, zone: DockZone) => void
  onResetLayout: () => void
}) {
  return (
    <div className="flex flex-none items-center gap-0.5">
      <IconToggle
        icon={Columns2}
        label={split ? 'Close split view' : 'Show two chats side by side'}
        pressed={split}
        onClick={onToggleSplit}
      />

      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <button
            type="button"
            aria-label="Panels and layout"
            title="Panels and layout"
            className={cn(
              iconButtonClass,
              // The house pattern for a selected control is border-primary/bg-soft;
              // a hover-only style would make an on toggle look unset.
              dock && 'border-primary/60 bg-soft text-accent-text'
            )}
          >
            <PanelsTopLeft size={14} aria-hidden />
          </button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align="end" className="w-64">
          {dock ? (
            <>
              {ALL_DOCK_ZONES.map((zone) => (
                <DropdownMenuItem key={zone} onSelect={() => onMoveDock(zone)}>
                  Move {dockKindLabel(dock.kind).toLowerCase()} to {zoneLabel(zone).toLowerCase()}
                </DropdownMenuItem>
              ))}
              <DropdownMenuSeparator />
              <DropdownMenuItem onSelect={onUnpinDock}>Unpin this panel</DropdownMenuItem>
            </>
          ) : (
            // Every zone, not just the right: the right edge is unavailable
            // below a window width the moving path already knows about, and
            // bottom and left are safe there.
            ALL_DOCK_KINDS.map((kind) => (
              <DropdownMenuSub key={kind}>
                <DropdownMenuSubTrigger>Pin {dockKindLabel(kind).toLowerCase()}…</DropdownMenuSubTrigger>
                <DropdownMenuSubContent>
                  {ALL_DOCK_ZONES.map((zone) => (
                    <DropdownMenuItem key={zone} onSelect={() => onPinDock(kind, zone)}>
                      {zoneLabel(zone)}
                    </DropdownMenuItem>
                  ))}
                </DropdownMenuSubContent>
              </DropdownMenuSub>
            ))
          )}
          <DropdownMenuSeparator />
          <DropdownMenuCheckboxItem checked={sourceBarsVisible} onCheckedChange={onToggleSourceBars}>
            Show where chats started
            <span className="ml-auto pl-3 text-2xs text-muted-foreground">Ctrl+Alt+S</span>
          </DropdownMenuCheckboxItem>
          <DropdownMenuSeparator />
          <DropdownMenuItem onSelect={onResetLayout}>
            <RotateCcw size={13} aria-hidden />
            Reset workspace layout…
          </DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>
    </div>
  )
}

const iconButtonClass = cn(
  'flex h-[26px] w-[26px] flex-none items-center justify-center rounded border border-transparent text-sub',
  'hover:border-border hover:bg-panel3 hover:text-foreground'
)

function IconToggle({
  icon: Icon,
  label,
  pressed,
  onClick,
}: {
  icon: React.ComponentType<{ size?: number; className?: string; 'aria-hidden'?: boolean }>
  label: string
  pressed: boolean
  onClick: () => void
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      aria-pressed={pressed}
      aria-label={label}
      title={label}
      className={cn(iconButtonClass, pressed && 'border-primary/60 bg-soft text-accent-text')}
    >
      <Icon size={14} aria-hidden />
    </button>
  )
}
