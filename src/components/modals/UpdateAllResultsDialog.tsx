import { useEffect, useMemo, useState } from 'react'
import {
  ChevronDown,
  ChevronRight,
  CircleCheck,
  Info,
  KeyRound,
  OctagonX,
  TriangleAlert,
} from 'lucide-react'
import { Button } from '@/components/ui/button'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip'
import { useOpenRepo } from '@/hooks/useRepoActions'
import type { BranchUpdate, RepoUpdate } from '@/lib/bindings'
import { plural } from '@/lib/gitDisplay'
import { pathKey } from '@/lib/paths'
import {
  branchUpdateText,
  branchUpdateTone,
  groupRepoUpdates,
  updateDetail,
  updateHeadline,
  updateTotals,
  type ResultSection,
} from '@/lib/updateAll'
import { cn } from '@/lib/utils'
import { startUpdateAll, useUpdateAllStore } from '@/stores/updateAllStore'

const SET_ASIDE_WARNING =
  'Your unsaved changes are put to one side, the branch is updated, and then your changes are put back. ' +
  'If your changes clash with the new commits, that is only noted here - you will need to open the project to sort it out yourself.'

const SECTION: Record<ResultSection, { title: string; icon: React.ReactNode; defaultOpen: boolean }> = {
  error: {
    title: 'Could not update',
    icon: <OctagonX aria-hidden size={14} className="text-removed" />,
    defaultOpen: true,
  },
  warning: {
    title: 'Needs a look',
    icon: <TriangleAlert aria-hidden size={14} className="text-modified" />,
    defaultOpen: true,
  },
  updated: {
    title: 'Updated',
    icon: <CircleCheck aria-hidden size={14} className="text-added" />,
    defaultOpen: true,
  },
  unchanged: {
    title: 'Already up to date',
    icon: <CircleCheck aria-hidden size={14} className="text-muted-foreground" />,
    defaultOpen: false,
  },
  skipped: {
    title: 'Not checked - the update was stopped first',
    icon: <Info aria-hidden size={14} className="text-muted-foreground" />,
    defaultOpen: false,
  },
}

function BranchLine({ branch }: { branch: BranchUpdate }) {
  const tone = branchUpdateTone(branch)
  return (
    <li className="flex items-start gap-2 text-2xs leading-4">
      <span
        aria-hidden
        className={cn(
          'mt-1.5 size-1.5 flex-none rounded-full',
          tone === 'ok' && 'bg-added',
          tone === 'warning' && 'bg-modified',
          tone === 'error' && 'bg-removed',
          tone === 'info' && 'bg-muted-foreground',
        )}
      />
      <span className="min-w-0">
        <span className="font-mono text-foreground">{branch.name}</span>
        <span className="text-muted-foreground"> - {branchUpdateText(branch)}</span>
      </span>
    </li>
  )
}

function RepoRow({
  repo,
  showPath,
  ticked,
  onTick,
  onOpen,
}: {
  repo: RepoUpdate
  showPath: boolean
  ticked: boolean
  onTick: (on: boolean) => void
  onOpen: (() => void) | null
}) {
  return (
    <div className="rounded-md border border-border bg-panel px-3 py-2.5">
      <div className="flex items-start gap-2.5">
        <div className="min-w-0 flex-1">
          <div className="flex items-baseline gap-2">
            <span className="truncate text-xs font-semibold text-foreground">{repo.name}</span>
            {repo.commits_received > 0 && (
              <span className="flex-none rounded-full bg-added/15 px-1.5 font-mono text-2xs text-added">
                +{repo.commits_received}
              </span>
            )}
          </div>
          {showPath && <div className="truncate text-2xs text-muted-foreground">{repo.path}</div>}
          {repo.message && (
            <p className={cn('mt-1 text-2xs', repo.level === 'error' ? 'text-removed' : 'text-modified')}>
              {repo.needs_sign_in && <KeyRound aria-hidden size={11} className="mr-1 inline" />}
              {repo.message}
            </p>
          )}
          {repo.branches.length > 0 && (
            <ul className="mt-1.5 grid gap-1">
              {repo.branches.map((branch) => (
                <BranchLine key={branch.name} branch={branch} />
              ))}
            </ul>
          )}
          {repo.changes_in_the_way && (
            <Tooltip>
              <TooltipTrigger asChild>
                <label className="mt-2 flex w-fit cursor-pointer items-center gap-1.5 text-2xs text-foreground">
                  <input
                    type="checkbox"
                    checked={ticked}
                    onChange={(e) => onTick(e.target.checked)}
                    className="size-3.5 cursor-pointer accent-[var(--gw-accent)]"
                  />
                  Set my changes aside and update
                  <Info aria-hidden size={11} className="text-muted-foreground" />
                </label>
              </TooltipTrigger>
              <TooltipContent side="right">{SET_ASIDE_WARNING}</TooltipContent>
            </Tooltip>
          )}
        </div>
        {onOpen && (
          <Button variant="ghost" size="sm" className="h-7 flex-none text-2xs" onClick={onOpen}>
            Open
          </Button>
        )}
      </div>
    </div>
  )
}

/**
 * Everything the last "get the latest" run found, worst first. Projects
 * skipped for unsaved changes can be ticked and run again with those changes
 * set aside; projects that wanted a sign-in can be run again with sign-in
 * windows allowed.
 */
export function UpdateAllResultsDialog() {
  const open = useUpdateAllStore((s) => s.resultsOpen)
  const results = useUpdateAllStore((s) => s.results)
  const running = useUpdateAllStore((s) => s.progress != null)
  const closeResults = useUpdateAllStore((s) => s.closeResults)
  const openRepo = useOpenRepo()
  const [ticked, setTicked] = useState<Set<string>>(new Set())
  const [collapsed, setCollapsed] = useState<Partial<Record<ResultSection, boolean>>>({})

  useEffect(() => {
    if (!open) {
      setTicked(new Set())
      setCollapsed({})
    }
  }, [open])

  const groups = useMemo(() => groupRepoUpdates(results?.repos ?? []), [results])
  const totals = useMemo(() => updateTotals(results?.repos ?? []), [results])
  const signInPaths = useMemo(
    () => (results?.repos ?? []).filter((r) => r.needs_sign_in).map((r) => r.path),
    [results],
  )
  const tickedPaths = useMemo(
    () => (results?.repos ?? []).filter((r) => r.changes_in_the_way && ticked.has(pathKey(r.path))).map((r) => r.path),
    [results, ticked],
  )

  if (!results) return null
  const showPaths = results.scope === 'all'
  const detail = updateDetail(totals, results.cancelled, results.scope)

  const tick = (repo: RepoUpdate, on: boolean) => {
    const next = new Set(ticked)
    if (on) next.add(pathKey(repo.path))
    else next.delete(pathKey(repo.path))
    setTicked(next)
  }

  const retrySetAside = async () => {
    const started = await startUpdateAll(
      { folders: [], paths: tickedPaths, allow_set_aside: tickedPaths, sign_in: false },
      true,
    )
    if (started) setTicked(new Set())
  }

  const retrySignIn = () =>
    void startUpdateAll({ folders: [], paths: signInPaths, allow_set_aside: [], sign_in: true }, true)

  const isCollapsed = (section: ResultSection) => collapsed[section] ?? !SECTION[section].defaultOpen

  return (
    <Dialog open={open} onOpenChange={(next) => !next && closeResults()}>
      <DialogContent className="flex max-h-[min(760px,calc(100vh-64px))] flex-col gap-0 overflow-hidden p-0 sm:max-w-2xl">
        <DialogHeader className="border-b border-border px-5 py-4 pr-12">
          <DialogTitle className="text-sm">{updateHeadline(totals)}</DialogTitle>
          <DialogDescription className="text-2xs">
            {showPaths ? `Checked ${plural(totals.projects, 'project')}` : 'Checked every branch in this project'}
            {detail && ` · ${detail}`}
            {' · '}
            {new Date(results.finishedAt).toLocaleTimeString([], { hour: 'numeric', minute: '2-digit' })}
          </DialogDescription>
        </DialogHeader>

        <div className="min-h-0 flex-1 space-y-4 overflow-y-auto px-5 py-4">
          {groups.length === 0 && (
            <p className="py-10 text-center text-xs text-muted-foreground">No projects were found to update.</p>
          )}
          {groups.map(({ section, repos }) => {
            const meta = SECTION[section]
            const hidden = isCollapsed(section)
            return (
              <section key={section}>
                <button
                  type="button"
                  onClick={() => setCollapsed({ ...collapsed, [section]: !hidden })}
                  className="mb-2 flex w-full items-center gap-2 text-left text-xs font-semibold text-foreground"
                  aria-expanded={!hidden}
                >
                  {hidden ? <ChevronRight aria-hidden size={13} /> : <ChevronDown aria-hidden size={13} />}
                  {meta.icon}
                  {meta.title}
                  <span className="font-normal text-muted-foreground">{repos.length}</span>
                </button>
                {!hidden && (
                  <div className="grid gap-1.5">
                    {repos.map((repo) => (
                      <RepoRow
                        key={repo.path}
                        repo={repo}
                        showPath={showPaths}
                        ticked={ticked.has(pathKey(repo.path))}
                        onTick={(on) => tick(repo, on)}
                        onOpen={
                          showPaths
                            ? () => {
                                closeResults()
                                openRepo.mutate(repo.path)
                              }
                            : null
                        }
                      />
                    ))}
                  </div>
                )}
              </section>
            )
          })}
        </div>

        <div className="flex flex-wrap items-center gap-2 border-t border-border px-5 py-3">
          <p className="min-w-0 flex-1 text-2xs text-muted-foreground">
            {totals.changesInTheWay > 0
              ? 'Projects with unsaved changes were left alone. Tick one to set the changes aside and update it.'
              : 'Branches with their own new work are never combined automatically.'}
          </p>
          {signInPaths.length > 0 && (
            <Button variant="secondary" size="sm" onClick={retrySignIn} disabled={running}>
              <KeyRound aria-hidden size={13} />
              Sign in and try again ({signInPaths.length})
            </Button>
          )}
          {totals.changesInTheWay > 0 && (
            <Tooltip>
              <TooltipTrigger asChild>
                <span>
                  <Button size="sm" onClick={() => void retrySetAside()} disabled={running || tickedPaths.length === 0}>
                    Update ticked ({tickedPaths.length})
                  </Button>
                </span>
              </TooltipTrigger>
              <TooltipContent>{SET_ASIDE_WARNING}</TooltipContent>
            </Tooltip>
          )}
          <Button variant="outline" size="sm" onClick={closeResults}>
            Close
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  )
}
