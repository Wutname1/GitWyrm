import { Download, FolderGit2, Loader2, Minus, Plus, RotateCcw, Search } from 'lucide-react'
import { useBranches, useStatus, useWorktrees } from '@/hooks/useGitQueries'
import { useOpenspecStatus } from '@/hooks/useOpenspec'
import { isActive, stateGlyph, stateLabel, useAiRun } from '@/hooks/useAiRun'
import { useUpdater } from '@/hooks/useUpdater'
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
import { Tooltip, TooltipContent, TooltipHint, TooltipTrigger } from '@/components/ui/tooltip'

function ZoomControl() {
  const uiScale = useWorkspaceStore((s) => s.uiScale)
  const setUiScale = useWorkspaceStore((s) => s.setUiScale)
  const percent = Math.round(uiScale * 100)
  const isDefault = uiScale === DEFAULT_UI_SCALE

  return (
    <div className="flex items-center gap-1">
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
        <TooltipContent side="top">
          {isDefault
            ? 'App zoom - click to make everything bigger or smaller'
            : `App zoom is ${percent}% - click to change it, or use the arrow to go back to 100%`}
        </TooltipContent>
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
              disabled={isDefault}
            >
              Reset to 100%
            </Button>
          </div>
        </PopoverContent>
      </Popover>
      </Tooltip>
      {!isDefault && (
        <Tooltip>
          <TooltipTrigger asChild>
            <button
              type="button"
              aria-label="Reset zoom to 100%"
              onClick={() => setUiScale(DEFAULT_UI_SCALE)}
              className="titlebar-no-drag flex items-center rounded px-1 text-sub hover:text-text"
            >
              <RotateCcw className="size-3" />
            </button>
          </TooltipTrigger>
          <TooltipContent side="top">Back to 100%</TooltipContent>
        </Tooltip>
      )}
    </div>
  )
}

/**
 * Spec counts for repos that use OpenSpec, and whether the OpenSpec tool is
 * around. Absent entirely for every other repo. The tool only matters for the
 * spec check and archiving, so its absence is stated quietly rather than as a
 * problem -- reading and ticking work either way.
 */
/**
 * The run's state, wherever the user is in the app.
 *
 * Its own segment rather than a suffix on the openspec count: a gate is
 * something to act on, and appending it to a tally is how it gets missed.
 * Silent when nothing is running.
 */
function AiRunSegment() {
  const repo = useActiveRepo()
  const run = useAiRun(repo?.id ?? null)
  if (!run.state || !run.session) return null

  const needsYou = run.state === 'needsYou'
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <span className={needsYou ? 'text-[var(--gw-amber)]' : 'text-muted-foreground'}>
          {stateGlyph(run.state)} {stateLabel(run.state)}
        </span>
      </TooltipTrigger>
      <TooltipContent side="top">
        Task {run.session.task_number} · {run.session.task_text}
        {run.latest ? ` — ${run.latest}` : ''}
        {isActive(run.state) ? '' : ' (this run has ended)'}
      </TooltipContent>
    </Tooltip>
  )
}

function OpenspecSegment() {
  const repo = useActiveRepo()
  const status = useOpenspecStatus(repo?.id ?? null)
  const data = status.data

  if (!data?.present) return null

  const cliLabel = data.cli.available
    ? `tool ${data.cli.version ?? 'ready'}`
    : 'tool not installed'

  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <span className="text-muted-foreground">
          openspec · {data.active_count} active
        </span>
      </TooltipTrigger>
      <TooltipContent side="top">
        {data.active_count} {data.active_count === 1 ? 'change' : 'changes'} in progress
        {data.archived_count > 0 ? `, ${data.archived_count} archived` : ''} · {cliLabel}
        {data.cli.available ? '' : ' (only the spec check and archiving need it)'}
      </TooltipContent>
    </Tooltip>
  )
}

/**
 * Centered "Update now" pill, shown only once a newer release is detected.
 * Downloads, installs, and relaunches when clicked.
 */
function UpdateButton() {
  const state = useUpdater((s) => s.state)
  const version = useUpdater((s) => s.version)
  const install = useUpdater((s) => s.install)

  if (state !== 'available' && state !== 'downloading' && state !== 'ready') return null

  const busy = state === 'downloading' || state === 'ready'

  return (
    <div className="pointer-events-none absolute inset-x-0 flex justify-center">
      <button
        type="button"
        onClick={() => install()}
        disabled={busy}
        className="titlebar-no-drag pointer-events-auto flex h-[18px] items-center gap-1.5 rounded-full bg-blue-600 px-3 font-sans text-2xs font-semibold text-white shadow-sm transition-colors hover:bg-blue-500 disabled:opacity-80"
      >
        {busy ? (
          <>
            <Loader2 className="size-3 animate-spin" />
            {state === 'ready' ? 'Restarting…' : `Downloading ${version ?? ''}…`}
          </>
        ) : (
          <>
            <Download className="size-3" />
            Update now{version ? ` to ${version}` : ''}
          </>
        )}
      </button>
    </div>
  )
}

export function StatusBar() {
  const repo = useActiveRepo()
  const status = useStatus(repo?.id ?? null)
  const branches = useBranches(repo?.id ?? null)
  const { data: worktrees } = useWorktrees(repo?.id ?? null)

  // Name the checkout whenever it is not the project's original folder. With
  // sibling tabs of one project open, "which folder am I committing into" is
  // the question the status bar exists to answer, and a path alone makes the
  // reader work it out.
  const current = worktrees?.find((w) => w.is_current)
  const inWorktree = current != null && !current.is_main

  const head = branches.data?.local.find((b) => b.is_head)
  const sync = head ? branchSync(head) : null
  const total = (status.data?.staged.length ?? 0) + (status.data?.unstaged.length ?? 0)

  return (
    <div data-dim-on-drag className="relative flex h-6 flex-none items-center gap-4 border-t border-border bg-panel2 px-3 font-mono text-2xs text-sub">
      {sync?.text ? (
        sync.title ? (
          <TooltipHint label={sync.title}>
            <span>{sync.text}</span>
          </TooltipHint>
        ) : (
          <span>{sync.text}</span>
        )
      ) : null}
      <span className="text-muted-foreground">{total} changes</span>
      <AiRunSegment />
      <OpenspecSegment />
      <div className="flex-1" />
      <UpdateButton />
      {inWorktree && current && (
        <TooltipHint
          label={`You're working in the ${current.folder_name} worktree — this project checked out in its own folder. Commits here don't touch the main checkout.`}
        >
          <span className="flex items-center gap-1 text-[var(--gw-accent-text)]">
            <FolderGit2 size={9} />
            {current.folder_name}
          </span>
        </TooltipHint>
      )}
      {repo && <span className="text-muted-foreground">{repo.path}</span>}
      <ZoomControl />
    </div>
  )
}
