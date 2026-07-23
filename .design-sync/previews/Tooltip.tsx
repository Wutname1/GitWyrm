import {
  Tooltip,
  TooltipTrigger,
  TooltipContent,
  TooltipButton,
  TooltipHint,
  Button,
} from 'gitwyrm-mockup'
import { RefreshCw, GitBranch, Upload, Download } from 'lucide-react'

export function OnToolbarButton() {
  return (
    <div style={{ padding: 40, display: 'flex', gap: 12, alignItems: 'center' }}>
      <Tooltip defaultOpen>
        <TooltipTrigger asChild>
          <Button size="icon" variant="ghost" aria-label="Fetch">
            <RefreshCw />
          </Button>
        </TooltipTrigger>
        <TooltipContent side="bottom">Fetch from origin</TooltipContent>
      </Tooltip>
    </div>
  )
}

export function TooltipButtonExample() {
  return (
    <div style={{ padding: 40, display: 'flex', gap: 12, alignItems: 'center' }}>
      <TooltipButton
        tooltip="Push commits to origin/main"
        tooltipSide="bottom"
        className="inline-flex size-9 items-center justify-center rounded-md border border-border text-foreground"
      >
        <Upload style={{ width: 16, height: 16 }} />
      </TooltipButton>
    </div>
  )
}

export function TooltipHintExample() {
  return (
    <div style={{ padding: 40, display: 'flex', gap: 12, alignItems: 'center' }}>
      <TooltipHint label="Pull 3 commits from origin" side="bottom">
        <Button variant="outline">
          <Download /> Pull
        </Button>
      </TooltipHint>
    </div>
  )
}

export function OnBranchName() {
  return (
    <div style={{ padding: 40, display: 'flex', gap: 12, alignItems: 'center' }}>
      <Tooltip defaultOpen>
        <TooltipTrigger asChild>
          <span className="inline-flex items-center gap-1.5 text-sm text-foreground">
            <GitBranch style={{ width: 14, height: 14 }} />
            feature/commit-graph
          </span>
        </TooltipTrigger>
        <TooltipContent side="top">Checked out 2 hours ago · 4 commits ahead of origin</TooltipContent>
      </Tooltip>
    </div>
  )
}
