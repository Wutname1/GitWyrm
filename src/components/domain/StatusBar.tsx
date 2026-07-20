import { Minus, Plus, Search } from 'lucide-react'
import { useBranches, useStatus } from '@/hooks/useGitQueries'
import { useActiveRepo } from '@/stores/workspaceStore'
import { branchSync } from '@/lib/branchActions'
import {
  DEFAULT_UI_SCALE,
  MAX_UI_SCALE,
  MIN_UI_SCALE,
  UI_SCALE_STEP,
  useWorkspaceStore,
} from '@/stores/workspaceStore'
import { Button } from '@/components/ui/button'
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover'
import { Slider } from '@/components/ui/slider'
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip'

function ZoomControl() {
  const uiScale = useWorkspaceStore((s) => s.uiScale)
  const setUiScale = useWorkspaceStore((s) => s.setUiScale)
  const percent = Math.round(uiScale * 100)

  return (
    <Tooltip>
      <Popover>
        <TooltipTrigger asChild>
          <PopoverTrigger asChild>
            <button
              type="button"
              aria-label="App zoom"
              className="titlebar-no-drag flex items-center gap-1 rounded px-1 text-sub hover:text-text"
            >
              <Search className="size-3" />
              <span>{percent}%</span>
            </button>
          </PopoverTrigger>
        </TooltipTrigger>
        <TooltipContent side="top">App zoom</TooltipContent>
        <PopoverContent align="end" side="top" collisionPadding={8} className="w-56">
          <div className="flex flex-col gap-3 text-text">
            <div className="flex items-center justify-between text-sm font-medium">
              <span>Zoom</span>
              <span className="font-mono text-sub">{percent}%</span>
            </div>
            <div className="flex items-center gap-2">
              <Button
                variant="outline"
                size="icon-sm"
                onClick={() => setUiScale(uiScale - UI_SCALE_STEP)}
                disabled={uiScale <= MIN_UI_SCALE}
                aria-label="Zoom out"
              >
                <Minus />
              </Button>
              <Slider
                value={[percent]}
                min={MIN_UI_SCALE * 100}
                max={MAX_UI_SCALE * 100}
                step={UI_SCALE_STEP * 100}
                onValueChange={([v]) => setUiScale(v / 100)}
                className="flex-1"
                aria-label="Zoom level"
              />
              <Button
                variant="outline"
                size="icon-sm"
                onClick={() => setUiScale(uiScale + UI_SCALE_STEP)}
                disabled={uiScale >= MAX_UI_SCALE}
                aria-label="Zoom in"
              >
                <Plus />
              </Button>
            </div>
            <Button
              variant="ghost"
              size="sm"
              onClick={() => setUiScale(DEFAULT_UI_SCALE)}
              disabled={uiScale === DEFAULT_UI_SCALE}
            >
              Reset to 100%
            </Button>
          </div>
        </PopoverContent>
      </Popover>
    </Tooltip>
  )
}

export function StatusBar() {
  const repo = useActiveRepo()
  const status = useStatus(repo?.id ?? null)
  const branches = useBranches(repo?.id ?? null)

  const head = branches.data?.local.find((b) => b.is_head)
  const sync = head ? branchSync(head) : null
  const total = (status.data?.staged.length ?? 0) + (status.data?.unstaged.length ?? 0)

  return (
    <div data-dim-on-drag className="flex h-6 flex-none items-center gap-4 border-t border-border bg-panel2 px-3 font-mono text-[10.5px] text-sub">
      {sync?.text ? <span title={sync.title ?? undefined}>{sync.text}</span> : null}
      <span className="text-muted-foreground">{total} changes</span>
      <div className="flex-1" />
      {repo && <span className="text-muted-foreground">{repo.path}</span>}
      <ZoomControl />
    </div>
  )
}
