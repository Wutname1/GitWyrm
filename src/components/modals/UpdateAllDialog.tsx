import { useEffect, useMemo, useRef, useState, type ReactNode } from 'react'
import {
  ChevronDown,
  ChevronRight,
  CircleCheck,
  Clock,
  FolderSync,
  Info,
  Loader2,
  OctagonX,
  Square,
  TriangleAlert,
} from 'lucide-react'
import { Button } from '@/components/ui/button'
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from '@/components/ui/dialog'
import { useCodeFolderRepos } from '@/hooks/useRepoActions'
import type { RepoUpdate, RunningRepo, UpdateAllProgress } from '@/lib/bindings'
import { plural } from '@/lib/gitDisplay'
import { pathKey, pathName } from '@/lib/paths'
import {
  buildPickerSections,
  pickerPaths,
  repoUpdateSummary,
  sectionTick,
  SET_ASIDE_WARNING,
  updateDetail,
  updateHeadline,
  updateTotals,
  type PickerSection,
} from '@/lib/updateAll'
import { cn } from '@/lib/utils'
import { startUpdateAll, stopUpdateAll, useUpdateAllStore } from '@/stores/updateAllStore'
import { useWorkspaceStore } from '@/stores/workspaceStore'

/** The picker's choices, remembered in this browser profile only. */
const PICKER_KEY = 'gitwyrm.updateAll.picker'

interface PickerMemory {
  /** Path keys the user unticked. Unticked rather than ticked, so new projects start ticked. */
  excluded: string[]
  setAside: boolean
  signIn: boolean
}

function readMemory(): PickerMemory {
  try {
    const raw = window.localStorage.getItem(PICKER_KEY)
    if (raw) {
      const parsed = JSON.parse(raw) as Partial<PickerMemory>
      return {
        excluded: Array.isArray(parsed.excluded) ? parsed.excluded : [],
        setAside: parsed.setAside === true,
        signIn: parsed.signIn === true,
      }
    }
  } catch {
    // Storage off or unreadable: start from everything ticked.
  }
  return { excluded: [], setAside: false, signIn: false }
}

function writeMemory(memory: PickerMemory) {
  try {
    window.localStorage.setItem(PICKER_KEY, JSON.stringify(memory))
  } catch {
    // Losing the memory only means ticking again next time.
  }
}

function TriCheckbox({
  state,
  onChange,
  label,
}: {
  state: 'all' | 'some' | 'none'
  onChange: (checked: boolean) => void
  label: string
}) {
  const ref = useRef<HTMLInputElement>(null)
  useEffect(() => {
    if (ref.current) ref.current.indeterminate = state === 'some'
  }, [state])
  return (
    <input
      ref={ref}
      type="checkbox"
      aria-label={label}
      checked={state === 'all'}
      onChange={(e) => onChange(e.target.checked)}
      onClick={(e) => e.stopPropagation()}
      className="size-3.5 flex-none cursor-pointer accent-[var(--gw-accent)]"
    />
  )
}

function Section({
  title,
  subtitle,
  count,
  icon,
  open,
  onToggle,
  control,
  children,
}: {
  title: string
  subtitle?: string | null
  count?: ReactNode
  icon?: ReactNode
  open: boolean
  onToggle: () => void
  control?: ReactNode
  children: ReactNode
}) {
  return (
    <section className="rounded-md border border-border bg-panel">
      <div className="flex items-center gap-2 px-3 py-2">
        {control}
        <button
          type="button"
          onClick={onToggle}
          aria-expanded={open}
          className="flex min-w-0 flex-1 items-center gap-2 text-left"
        >
          {icon}
          <span className="min-w-0 flex-1">
            <span className="block truncate text-xs font-semibold text-foreground">{title}</span>
            {subtitle && <span className="block truncate text-2xs text-muted-foreground">{subtitle}</span>}
          </span>
          {count != null && <span className="flex-none text-2xs text-muted-foreground">{count}</span>}
          {open ? (
            <ChevronDown aria-hidden size={13} className="flex-none text-muted-foreground" />
          ) : (
            <ChevronRight aria-hidden size={13} className="flex-none text-muted-foreground" />
          )}
        </button>
      </div>
      {open && <div className="border-t border-border px-3 py-2">{children}</div>}
    </section>
  )
}

/** Choose which projects to bring up to date, and how. */
function PickerView({ onClose }: { onClose: () => void }) {
  const openRepos = useWorkspaceStore((s) => s.openRepos)
  const scan = useCodeFolderRepos()
  const [memory, setMemory] = useState(readMemory)
  const [collapsed, setCollapsed] = useState<Record<string, boolean>>({})
  const [starting, setStarting] = useState(false)

  const sections = useMemo(() => buildPickerSections(openRepos, scan.folders), [openRepos, scan.folders])
  const allPaths = useMemo(() => pickerPaths(sections), [sections])
  const excluded = useMemo(() => new Set(memory.excluded), [memory.excluded])
  const chosen = allPaths.filter((p) => !excluded.has(pathKey(p)))

  const update = (next: Partial<PickerMemory>) => {
    const merged = { ...memory, ...next }
    setMemory(merged)
    writeMemory(merged)
  }
  const setTicked = (paths: string[], on: boolean) => {
    const next = new Set(excluded)
    for (const p of paths) {
      if (on) next.delete(pathKey(p))
      else next.add(pathKey(p))
    }
    update({ excluded: [...next] })
  }
  const toggleOpen = (key: string) => setCollapsed({ ...collapsed, [key]: !collapsed[key] })

  const start = async () => {
    setStarting(true)
    await startUpdateAll({
      folders: [],
      paths: chosen,
      allow_set_aside: memory.setAside ? chosen : [],
      sign_in: memory.signIn,
    })
    setStarting(false)
  }

  const renderSection = (section: PickerSection) => {
    const tick = sectionTick(section, excluded)
    const open = !collapsed[section.key]
    const on = section.repos.filter((r) => !excluded.has(pathKey(r.path))).length
    return (
      <Section
        key={section.key}
        title={section.title}
        subtitle={section.subtitle}
        count={section.unavailable ? 'Not available' : `${on} of ${section.repos.length}`}
        open={open}
        onToggle={() => toggleOpen(section.key)}
        control={
          <TriCheckbox
            state={tick}
            label={`All projects in ${section.title}`}
            onChange={(checked) =>
              setTicked(
                section.repos.map((r) => r.path),
                checked,
              )
            }
          />
        }
      >
        {section.unavailable ? (
          <p className="text-2xs text-muted-foreground">This folder could not be read. Is the drive plugged in?</p>
        ) : section.repos.length === 0 ? (
          <p className="text-2xs text-muted-foreground">No projects in this folder.</p>
        ) : (
          <ul className="grid gap-0.5">
            {section.repos.map((repo) => (
              <li key={repo.path}>
                <label className="flex cursor-pointer items-center gap-2 rounded px-1 py-1 hover:bg-panel3">
                  <input
                    type="checkbox"
                    checked={!excluded.has(pathKey(repo.path))}
                    onChange={(e) => setTicked([repo.path], e.target.checked)}
                    className="size-3.5 flex-none cursor-pointer accent-[var(--gw-accent)]"
                  />
                  <span className="min-w-0 flex-1 truncate text-xs text-foreground">{repo.name}</span>
                  {repo.branch && (
                    <span className="max-w-40 flex-none truncate font-mono text-2xs text-muted-foreground">
                      {repo.branch}
                    </span>
                  )}
                </label>
              </li>
            ))}
          </ul>
        )}
      </Section>
    )
  }

  return (
    <>
      <DialogHeader className="select-text border-b border-border px-5 py-4 pr-12">
        <DialogTitle className="flex items-center gap-2 text-sm">
          <FolderSync aria-hidden size={14} />
          Get the latest for…
        </DialogTitle>
        <DialogDescription className="text-2xs">
          Tick the projects to bring up to date. Every branch that is behind its server moves forward. Branches with
          their own new work are left alone.
        </DialogDescription>
      </DialogHeader>

      <div className="min-h-0 flex-1 space-y-2 select-text overflow-y-auto px-5 py-4">
        {sections.length === 0 && (
          <p className="py-10 text-center text-xs text-muted-foreground">
            {scan.isLoading ? 'Looking for your projects…' : 'Open a project or add a code folder first.'}
          </p>
        )}
        {sections.map(renderSection)}
      </div>

      {/* Outside the scrolling list, so the options stay in view however many
          projects there are. */}
      <div className="border-t border-border bg-panel px-5 py-3">
        <div className="mb-2 text-2xs font-semibold uppercase tracking-[.09em] text-sub">Options</div>
        <div className="grid gap-3">
          <label className="flex cursor-pointer items-start gap-2">
            <input
              type="checkbox"
              checked={memory.setAside}
              onChange={(e) => update({ setAside: e.target.checked })}
              className="mt-0.5 size-3.5 flex-none cursor-pointer accent-[var(--gw-accent)]"
            />
            <span>
              <span className="block text-xs text-foreground">Set my unsaved changes aside and put them back</span>
              <span className="block text-2xs text-muted-foreground">{SET_ASIDE_WARNING}</span>
            </span>
          </label>
          <label className="flex cursor-pointer items-start gap-2">
            <input
              type="checkbox"
              checked={memory.signIn}
              onChange={(e) => update({ signIn: e.target.checked })}
              className="mt-0.5 size-3.5 flex-none cursor-pointer accent-[var(--gw-accent)]"
            />
            <span>
              <span className="block text-xs text-foreground">Let sign-in windows open</span>
              <span className="block text-2xs text-muted-foreground">
                For projects whose server asks you to sign in. Projects are then updated one at a time, so windows never
                pile up.
              </span>
            </span>
          </label>
        </div>
      </div>

      <div className="flex flex-wrap items-center gap-2 border-t border-border px-5 py-3">
        <span className="text-2xs text-muted-foreground">
          {chosen.length} of {plural(allPaths.length, 'project')}
        </span>
        <Button variant="ghost" size="sm" className="h-7 text-2xs" onClick={() => setTicked(allPaths, true)}>
          Select all
        </Button>
        <Button variant="ghost" size="sm" className="h-7 text-2xs" onClick={() => setTicked(allPaths, false)}>
          Select none
        </Button>
        <div className="ml-auto flex items-center gap-2">
          <Button variant="outline" size="sm" onClick={onClose}>
            Cancel
          </Button>
          <Button size="sm" onClick={() => void start()} disabled={chosen.length === 0 || starting}>
            {starting ? 'Starting…' : `Get the latest (${chosen.length})`}
          </Button>
        </div>
      </div>
    </>
  )
}

/** Ticks once a second while mounted, for the elapsed times. */
function useNow(): number {
  const [now, setNow] = useState(() => Date.now())
  useEffect(() => {
    const id = window.setInterval(() => setNow(Date.now()), 1000)
    return () => window.clearInterval(id)
  }, [])
  return now
}

function elapsed(seconds: number): string {
  const s = Math.max(0, Math.floor(seconds))
  return s < 60 ? `${s}s` : `${Math.floor(s / 60)}m ${String(s % 60).padStart(2, '0')}s`
}

const LEVEL_ICON: Record<RepoUpdate['level'], ReactNode> = {
  error: <OctagonX aria-hidden size={13} className="flex-none text-removed" />,
  warning: <TriangleAlert aria-hidden size={13} className="flex-none text-modified" />,
  updated: <CircleCheck aria-hidden size={13} className="flex-none text-added" />,
  unchanged: <CircleCheck aria-hidden size={13} className="flex-none text-muted-foreground" />,
}

/** The run as it happens: what is being worked on, what finished, what is still waiting. */
function ProgressView({ progress, onClose }: { progress: UpdateAllProgress; onClose: () => void }) {
  const now = useNow()
  const [collapsed, setCollapsed] = useState<Record<string, boolean>>({
    waiting: true,
  })
  const toggle = (key: string) => setCollapsed({ ...collapsed, [key]: !collapsed[key] })

  const finishedKeys = new Set(progress.finished.map((r) => pathKey(r.path)))
  const runningKeys = new Set(progress.running.map((r) => pathKey(r.path)))
  const waiting = progress.queued.filter((p) => !finishedKeys.has(pathKey(p)) && !runningKeys.has(pathKey(p)))
  const finished = [...progress.finished].reverse()
  const pct = progress.total > 0 ? Math.round((progress.done / progress.total) * 100) : 0

  const stepText = (repo: RunningRepo) => (repo.step === 'fetching' ? 'Checking the server' : 'Moving branches forward')

  return (
    <>
      <DialogHeader className="select-text border-b border-border px-5 py-4 pr-12">
        <DialogTitle className="flex items-center gap-2 text-sm">
          <Loader2 aria-hidden size={14} className="animate-spin text-accent-text" />
          {progress.stopping
            ? 'Stopping…'
            : progress.total === 0
              ? 'Finding your projects…'
              : `Getting the latest · ${progress.done} of ${plural(progress.total, 'project')}`}
        </DialogTitle>
        <DialogDescription asChild>
          <div className="grid gap-1.5 text-2xs">
            <div
              role="progressbar"
              aria-valuemin={0}
              aria-valuemax={progress.total}
              aria-valuenow={progress.done}
              className="h-1.5 w-full overflow-hidden rounded-full bg-panel3"
            >
              <div
                className="h-full rounded-full bg-accent-text transition-[width] duration-300"
                style={{ width: progress.total > 0 ? `${pct}%` : '8%' }}
              />
            </div>
            <span>
              {plural(progress.branches_updated, 'branch', 'branches')} updated ·{' '}
              {plural(progress.commits_received, 'new commit')}
              {progress.errors + progress.warnings > 0 &&
                ` · ${plural(progress.errors + progress.warnings, 'project')} need${progress.errors + progress.warnings === 1 ? 's' : ''} a look`}
            </span>
          </div>
        </DialogDescription>
      </DialogHeader>

      <div className="min-h-0 flex-1 space-y-2 select-text overflow-y-auto px-5 py-4">
        <Section
          title="Working on now"
          count={progress.running.length}
          icon={<Loader2 aria-hidden size={13} className="animate-spin text-accent-text" />}
          open={!collapsed.running}
          onToggle={() => toggle('running')}
        >
          {progress.running.length === 0 ? (
            <p className="text-2xs text-muted-foreground">
              {progress.stopping ? 'Winding down…' : 'Nothing right now.'}
            </p>
          ) : (
            <ul className="grid gap-1">
              {progress.running.map((repo) => (
                <li key={repo.path} className="flex items-center gap-2 text-xs">
                  <span className="min-w-0 flex-1 truncate text-foreground">{pathName(repo.path)}</span>
                  <span className="flex-none text-2xs text-muted-foreground">
                    {stepText(repo)} · {elapsed(now / 1000 - repo.started_at)}
                  </span>
                </li>
              ))}
            </ul>
          )}
        </Section>

        <Section
          title="Finished"
          count={finished.length}
          icon={<CircleCheck aria-hidden size={13} className="text-added" />}
          open={!collapsed.finished}
          onToggle={() => toggle('finished')}
        >
          {finished.length === 0 ? (
            <p className="text-2xs text-muted-foreground">Nothing finished yet.</p>
          ) : (
            <ul className="grid gap-1">
              {finished.map((repo) => (
                <li key={repo.path} className="flex items-start gap-2 text-xs">
                  <span className="mt-0.5">
                    {repo.skipped ? (
                      <Info aria-hidden size={13} className="text-muted-foreground" />
                    ) : (
                      LEVEL_ICON[repo.level]
                    )}
                  </span>
                  <span className="min-w-0 flex-1">
                    <span className="block truncate text-foreground">{repo.name}</span>
                    <span
                      className={cn(
                        'block text-2xs',
                        repo.level === 'error'
                          ? 'text-removed'
                          : repo.level === 'warning'
                            ? 'text-modified'
                            : 'text-muted-foreground',
                      )}
                    >
                      {repoUpdateSummary(repo)}
                    </span>
                  </span>
                </li>
              ))}
            </ul>
          )}
        </Section>

        <Section
          title="Waiting"
          count={waiting.length}
          icon={<Clock aria-hidden size={13} className="text-muted-foreground" />}
          open={!collapsed.waiting}
          onToggle={() => toggle('waiting')}
        >
          {waiting.length === 0 ? (
            <p className="text-2xs text-muted-foreground">Every project has been started.</p>
          ) : (
            <ul className="grid gap-0.5">
              {waiting.map((path) => (
                <li key={path} className="truncate text-xs text-muted-foreground">
                  {pathName(path)}
                </li>
              ))}
            </ul>
          )}
        </Section>
      </div>

      <div className="flex flex-wrap items-center gap-2 border-t border-border px-5 py-3">
        <p className="min-w-0 flex-1 text-2xs text-muted-foreground">Closing this window does not stop the update.</p>
        <Button variant="outline" size="sm" onClick={onClose}>
          Close
        </Button>
        <Button variant="destructive" size="sm" onClick={() => void stopUpdateAll()} disabled={progress.stopping}>
          <Square aria-hidden size={12} />
          {progress.stopping ? 'Stopping…' : 'Stop'}
        </Button>
      </div>
    </>
  )
}

/** Shown in place of the live view when the run it was watching ends. */
function DoneView({ onClose }: { onClose: () => void }) {
  const results = useUpdateAllStore((s) => s.results)
  const openResults = useUpdateAllStore((s) => s.openResults)
  const totals = updateTotals(results?.repos ?? [])
  const detail = updateDetail(totals, results?.cancelled ?? false)
  return (
    <>
      <DialogHeader className="select-text border-b border-border px-5 py-4 pr-12">
        <DialogTitle className="flex items-center gap-2 text-sm">
          <CircleCheck aria-hidden size={14} className="text-added" />
          {updateHeadline(totals)}
        </DialogTitle>
        <DialogDescription className="text-2xs">{detail || 'Finished.'}</DialogDescription>
      </DialogHeader>
      <div className="flex items-center justify-end gap-2 px-5 py-3">
        <Button variant="outline" size="sm" onClick={onClose}>
          Close
        </Button>
        <Button size="sm" onClick={openResults}>
          See results
        </Button>
      </div>
    </>
  )
}

/**
 * "Get the latest for...": a picker when nothing is running, the live
 * progress of the run (with Stop) when something is, and a short summary when
 * the run it was watching ends.
 */
export function UpdateAllDialog() {
  const open = useUpdateAllStore((s) => s.dialogOpen)
  const progress = useUpdateAllStore((s) => s.progress)
  const closeDialog = useUpdateAllStore((s) => s.closeDialog)
  // Set once this window has shown a run, so its end reads as "done" rather
  // than dropping back to the picker as if nothing happened.
  const [watched, setWatched] = useState(false)

  useEffect(() => {
    if (open && progress) setWatched(true)
    if (!open) setWatched(false)
  }, [open, progress])

  const view = progress ? (
    <ProgressView progress={progress} onClose={closeDialog} />
  ) : watched ? (
    <DoneView onClose={closeDialog} />
  ) : (
    <PickerView onClose={closeDialog} />
  )

  return (
    <Dialog open={open} onOpenChange={(next) => !next && closeDialog()}>
      <DialogContent className="flex max-h-[min(760px,calc(100vh-64px))] flex-col gap-0 overflow-hidden p-0 sm:max-w-xl">
        {view}
      </DialogContent>
    </Dialog>
  )
}
