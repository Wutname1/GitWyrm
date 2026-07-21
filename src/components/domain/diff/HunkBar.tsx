import { cn } from '@/lib/utils'
import { TooltipButton } from '@/components/ui/tooltip'

interface HunkBarProps {
  text: string
  canPatch: boolean
  kind: 'staged' | 'unstaged'
  disabled?: boolean
  onApply: () => void
  onDiscard?: () => void
}

export function HunkBar({ text, canPatch, kind, disabled, onApply, onDiscard }: HunkBarProps) {
  const applyLabel = kind === 'staged' ? 'Unstage hunk' : 'Stage hunk'
  return (
    <div className="flex min-w-max items-center gap-2 bg-panel2 pr-3">
      <span className="w-[72px] flex-none select-none" />
      <span className="flex-1 whitespace-pre py-px font-mono text-2xs italic text-muted-foreground">
        {text}
      </span>
      {canPatch && (
        <div className="flex flex-none items-center gap-1">
          <button
            disabled={disabled}
            onClick={onApply}
            className={cn(
              'rounded border px-1.5 py-px text-2xs font-semibold disabled:opacity-50',
              kind === 'staged'
                ? 'border-border bg-panel3 text-sub hover:bg-panel'
                : 'border-primary/50 bg-soft text-accent-text hover:border-primary hover:bg-primary hover:text-primary-foreground'
            )}
          >
            {applyLabel}
          </button>
          {onDiscard && (
            <TooltipButton
              disabled={disabled}
              onClick={onDiscard}
              tooltip="Discard this hunk from the working tree"
              className="rounded border border-border bg-panel3 px-1.5 py-px text-2xs font-semibold text-removed hover:border-removed/60 hover:bg-removed/10 disabled:opacity-50"
            >
              Discard
            </TooltipButton>
          )}
        </div>
      )}
    </div>
  )
}
