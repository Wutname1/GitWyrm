import { X } from 'lucide-react'
import { cn } from '@/lib/utils'
import { Button } from '@/components/ui/button'
import { TooltipButton } from '@/components/ui/tooltip'

interface LineSelectionBarProps {
  count: number
  kind: 'staged' | 'unstaged'
  disabled?: boolean
  onApply: () => void
  onDiscard?: () => void
  onClear: () => void
}

export function LineSelectionBar({
  count,
  kind,
  disabled,
  onApply,
  onDiscard,
  onClear,
}: LineSelectionBarProps) {
  const applyLabel = kind === 'staged' ? 'Unstage lines' : 'Stage lines'
  return (
    <div className="flex flex-none items-center gap-2 border-t border-primary/40 bg-panel px-3.5 py-2">
      <span className="font-mono text-[11px] text-primary">
        {count} line{count === 1 ? '' : 's'} selected
      </span>
      <div className="ml-auto flex items-center gap-1.5">
        {onDiscard && (
          <Button
            variant="ghost"
            size="sm"
            disabled={disabled}
            onClick={onDiscard}
            className="h-auto rounded px-2 py-0.5 text-[11px] text-removed hover:bg-removed/10"
          >
            Discard
          </Button>
        )}
        <Button
          size="sm"
          disabled={disabled}
          onClick={onApply}
          className={cn('h-auto rounded px-2.5 py-0.5 text-[11px] font-semibold')}
        >
          {applyLabel}
        </Button>
        <TooltipButton
          onClick={onClear}
          tooltip="Clear selection"
          className="flex size-6 items-center justify-center rounded-[5px] border border-border bg-panel2 text-sub hover:bg-panel3"
        >
          <X size={12} />
        </TooltipButton>
      </div>
    </div>
  )
}
