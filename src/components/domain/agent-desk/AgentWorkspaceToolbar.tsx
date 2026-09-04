import { Columns2, PanelTopClose, PanelTopOpen, PanelsTopLeft, RotateCcw } from 'lucide-react'
import {
  DropdownMenu,
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
 * The workspace bar above the conversation panes (mockup's
 * `.ag-workspace-bar`): Split View, source-bar visibility, and the panel
 * menu, plus Reset workspace layout.
 *
 * Structure follows the mockup; colours and type come from the app's own
 * `--gw-*` tokens rather than the mockup's `--ag-*` palette. Every button
 * carries a pressed state and a label that says what the *current* state is,
 * matching the mockup's swapping button text (tasks.md 5.1, 8.1) -- so the
 * bar always reports the truth rather than only offering a verb.
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
  hideLabels,
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
  /** Icons only, at compact/narrow widths (mockup's 760px rule). */
  hideLabels: boolean
}) {
  const SourceIcon = sourceBarsVisible ? PanelTopClose : PanelTopOpen
  // The label reports the state, not the verb -- "Source bars hidden" while
  // they are hidden, matching the mockup's `data-toggle-source` text swap.
  const sourceLabel = sourceBarsVisible ? 'Source bars shown' : 'Source bars hidden'

  return (
    <div className="flex h-[35px] flex-none items-center gap-1.5 border-b border-border bg-panel pl-2.5 pr-1.5">
      <span className="flex-none text-2xs text-muted-foreground">Workspace</span>
      {!hideLabels && (
        <span className="min-w-0 truncate text-2xs text-sub">Chat clicks replace the active pane</span>
      )}
      <span className="flex-1" />

      <ToolbarButton
        icon={Columns2}
        label={split ? 'Split view on' : 'Split view'}
        pressed={split}
        hideLabel={hideLabels}
        onClick={onToggleSplit}
        title="Show two chats side by side (Ctrl+Alt+S toggles source bars)"
      />

      <ToolbarButton
        icon={SourceIcon}
        label={sourceLabel}
        pressed={!sourceBarsVisible}
        hideLabel={hideLabels}
        onClick={onToggleSourceBars}
        title="Hide or show the big source bar above each chat (Ctrl+Alt+S)"
      />

      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <button
            type="button"
            aria-label="Panels"
            className={cn(
              'flex h-[25px] flex-none items-center gap-1.5 rounded border border-transparent px-1.5 text-2xs text-sub',
              'hover:border-border hover:bg-panel3 hover:text-foreground',
              dock && 'border-border bg-panel3 text-foreground'
            )}
          >
            <PanelsTopLeft size={13} aria-hidden />
            {!hideLabels && <span>Panels</span>}
          </button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align="end" className="w-56">
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
            <>
              {/* Every zone, not just the right. All three items used to pin
                  to the right, and the right edge is unavailable below a
                  window width the moving path already knows about -- so on a
                  narrow window the only pinning affordance in the product
                  produced no visible change at all. Bottom and left are safe
                  at that width and were unreachable from a cold start. */}
              {ALL_DOCK_KINDS.map((kind) => (
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
              ))}
            </>
          )}
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

function ToolbarButton({
  icon: Icon,
  label,
  pressed,
  hideLabel,
  onClick,
  title,
}: {
  icon: React.ComponentType<{ size?: number; className?: string; 'aria-hidden'?: boolean }>
  label: string
  pressed: boolean
  hideLabel: boolean
  onClick: () => void
  title: string
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      aria-pressed={pressed}
      aria-label={label}
      title={title}
      className={cn(
        'flex h-[25px] flex-none items-center gap-1.5 rounded border border-transparent px-1.5 text-2xs text-sub',
        'hover:border-border hover:bg-panel3 hover:text-foreground',
        pressed && 'border-border bg-panel3 text-foreground'
      )}
    >
      <Icon size={13} aria-hidden />
      {!hideLabel && <span>{label}</span>}
    </button>
  )
}
