import type { ReactNode } from 'react'
import { MinusCircle, PlusCircle, Trash2 } from 'lucide-react'
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuLabel,
  ContextMenuSeparator,
  ContextMenuTrigger,
} from '@/components/ui/context-menu'

interface DiffLineMenuProps {
  /** 'staged' shows Unstage; 'unstaged' shows Stage + Discard. */
  kind: 'staged' | 'unstaged'
  /** How many lines the action will affect (1, or the whole selection). */
  count: number
  disabled?: boolean
  onOpenChange: (open: boolean) => void
  onApply: () => void
  onDiscard: () => void
  children: ReactNode
}

export function DiffLineMenu({
  kind,
  count,
  disabled,
  onOpenChange,
  onApply,
  onDiscard,
  children,
}: DiffLineMenuProps) {
  const noun = count === 1 ? 'this line' : `${count} lines`
  return (
    <ContextMenu onOpenChange={onOpenChange}>
      <ContextMenuTrigger asChild>{children}</ContextMenuTrigger>
      <ContextMenuContent className="w-48">
        <ContextMenuLabel className="text-[11px] text-sub">
          {count === 1 ? '1 line' : `${count} lines`}
        </ContextMenuLabel>
        <ContextMenuSeparator />
        {kind === 'staged' ? (
          <ContextMenuItem disabled={disabled} onSelect={onApply}>
            <MinusCircle />
            Unstage {noun}
          </ContextMenuItem>
        ) : (
          <ContextMenuItem disabled={disabled} onSelect={onApply}>
            <PlusCircle />
            Stage {noun}
          </ContextMenuItem>
        )}
        {kind === 'unstaged' && (
          <>
            <ContextMenuSeparator />
            <ContextMenuItem variant="destructive" disabled={disabled} onSelect={onDiscard}>
              <Trash2 />
              Discard {noun}
            </ContextMenuItem>
          </>
        )}
      </ContextMenuContent>
    </ContextMenu>
  )
}
