import {
  ContextMenu,
  ContextMenuTrigger,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuLabel,
  ContextMenuGroup,
  ContextMenuSeparator,
  ContextMenuShortcut,
  ContextMenuSub,
  ContextMenuSubTrigger,
  ContextMenuSubContent,
} from 'gitwyrm-mockup'
import { Copy, GitBranch, RotateCcw, Tag, GitMerge, Trash2 } from 'lucide-react'

// ContextMenu Root is controllable via `open`. Force it open with modal={false}
// so the menu content renders statically for the preview card.
export function CommitActions() {
  return (
    <div style={{ padding: 20 }}>
      <ContextMenu modal={false} open>
        <ContextMenuTrigger asChild>
          <div className="w-56 rounded-md border border-border px-3 py-2 text-sm text-foreground">
            f6cf72d · One-tap version buttons
          </div>
        </ContextMenuTrigger>
        <ContextMenuContent
          onEscapeKeyDown={(e) => e.preventDefault()}
          onPointerDownOutside={(e) => e.preventDefault()}
        >
          <ContextMenuLabel>Commit f6cf72d</ContextMenuLabel>
          <ContextMenuSeparator />
          <ContextMenuGroup>
            <ContextMenuItem>
              <Copy /> Copy SHA
              <ContextMenuShortcut>Ctrl+C</ContextMenuShortcut>
            </ContextMenuItem>
            <ContextMenuItem>
              <GitBranch /> Create branch here
            </ContextMenuItem>
            <ContextMenuItem>
              <Tag /> Tag this commit
            </ContextMenuItem>
          </ContextMenuGroup>
          <ContextMenuSeparator />
          <ContextMenuSub>
            <ContextMenuSubTrigger>
              <GitMerge /> Reset to here
            </ContextMenuSubTrigger>
            <ContextMenuSubContent>
              <ContextMenuItem>Soft</ContextMenuItem>
              <ContextMenuItem>Mixed</ContextMenuItem>
              <ContextMenuItem>Hard</ContextMenuItem>
            </ContextMenuSubContent>
          </ContextMenuSub>
          <ContextMenuItem>
            <RotateCcw /> Revert commit
          </ContextMenuItem>
          <ContextMenuSeparator />
          <ContextMenuItem variant="destructive">
            <Trash2 /> Drop commit
          </ContextMenuItem>
        </ContextMenuContent>
      </ContextMenu>
    </div>
  )
}
