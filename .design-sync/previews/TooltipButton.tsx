import {
  TooltipButton,
  Tooltip,
  TooltipTrigger,
  TooltipContent,
} from 'gitwyrm-mockup'
import { Download, RefreshCw, Upload } from 'lucide-react'

const iconBtn =
  'inline-flex size-9 items-center justify-center rounded-md border border-border text-foreground hover:bg-secondary'

// TooltipButton wraps its own Tooltip and cannot be forced open, so this story
// composes the same pieces with `defaultOpen` to show the open tooltip.
export function OpenTooltip() {
  return (
    <div style={{ padding: 48, display: 'flex', justifyContent: 'center' }}>
      <Tooltip defaultOpen>
        <TooltipTrigger asChild>
          <button className={iconBtn} aria-label="Push">
            <Upload style={{ width: 16, height: 16 }} />
          </button>
        </TooltipTrigger>
        <TooltipContent side="bottom">Push commits to origin/main</TooltipContent>
      </Tooltip>
    </div>
  )
}

export function ToolbarButtons() {
  return (
    <div style={{ padding: 24, display: 'flex', gap: 10, alignItems: 'center' }}>
      <TooltipButton tooltip="Fetch from origin" tooltipSide="bottom" className={iconBtn}>
        <RefreshCw style={{ width: 16, height: 16 }} />
      </TooltipButton>
      <TooltipButton tooltip="Pull from origin" tooltipSide="bottom" className={iconBtn}>
        <Download style={{ width: 16, height: 16 }} />
      </TooltipButton>
      <TooltipButton tooltip="Push to origin" tooltipSide="bottom" className={iconBtn}>
        <Upload style={{ width: 16, height: 16 }} />
      </TooltipButton>
    </div>
  )
}
