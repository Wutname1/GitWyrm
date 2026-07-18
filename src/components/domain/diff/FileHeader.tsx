import { X } from 'lucide-react'
import { cn } from '@/lib/utils'
import { TooltipButton } from '@/components/ui/tooltip'
import { useUiStore, type DiffRequest } from '@/stores/uiStore'

interface FileHeaderProps {
  request: DiffRequest
  additions: number
  deletions: number
}

export function FileHeader({ request, additions, deletions }: FileHeaderProps) {
  const closeDiff = useUiStore((s) => s.closeDiff)
  const pending = request.source.kind !== 'commit'

  const contextLabel =
    request.source.kind === 'staged'
      ? 'staged'
      : request.source.kind === 'unstaged'
        ? 'working tree'
        : `commit ${request.source.sha.slice(0, 7)}`

  return (
    <div className="flex h-10 flex-none items-center gap-2.5 border-b border-border bg-panel px-3.5">
      <span className="overflow-hidden text-ellipsis whitespace-nowrap font-mono text-xs text-foreground">
        {request.path}
      </span>
      <span
        className={cn(
          'flex-none rounded-full border px-[7px] py-px font-mono text-[9.5px] font-semibold',
          pending ? 'border-primary bg-soft text-primary' : 'border-border bg-panel3 text-sub'
        )}
      >
        {contextLabel}
      </span>
      <span className="font-mono text-[10.5px] text-added">+{additions}</span>
      <span className="font-mono text-[10.5px] text-removed">-{deletions}</span>
      <div className="flex-1" />
      <TooltipButton
        onClick={closeDiff}
        tooltip="Back to graph"
        className="flex size-6 flex-none items-center justify-center rounded-[5px] border border-border bg-panel2 text-xs text-sub hover:border-muted-foreground hover:bg-panel3"
      >
        <X size={12} />
      </TooltipButton>
    </div>
  )
}
