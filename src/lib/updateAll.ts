import type { BranchUpdate, RepoUpdate, RepoUpdateLevel } from '@/lib/bindings'
import { plural } from '@/lib/gitDisplay'
import { pathKey } from '@/lib/paths'

/** What one branch's result says, in plain words. */
export function branchUpdateText(branch: BranchUpdate): string {
  const waiting = plural(branch.commits, 'new commit')
  switch (branch.kind) {
    case 'updated':
      return `Got ${waiting}`
    case 'changes_in_the_way':
      return `${waiting} waiting. You have unsaved changes on this branch, so it was left alone.`
    case 'changes_clashed':
      return `Got ${waiting}, but your unsaved changes clash with them. Your changes are saved in your stashes. Open the project to sort it out.`
    case 'both_changed':
      return `Has its own new work and ${waiting} on the server. Open the project to combine them.`
    case 'open_elsewhere':
      return `${waiting} waiting. It is open in another working folder, so it was left alone.`
    case 'server_copy_gone':
      return 'Its copy on the server was deleted.'
    case 'failed':
      return branch.message ?? 'Could not be updated.'
  }
}

/** How serious one branch's result is, for its icon. */
export function branchUpdateTone(branch: BranchUpdate): 'error' | 'warning' | 'ok' | 'info' {
  switch (branch.kind) {
    case 'updated':
      return 'ok'
    case 'changes_clashed':
    case 'failed':
      return 'error'
    case 'changes_in_the_way':
    case 'both_changed':
    case 'open_elsewhere':
      return 'warning'
    case 'server_copy_gone':
      return 'info'
  }
}

export interface UpdateTotals {
  projects: number
  errors: number
  warnings: number
  updatedProjects: number
  branchesUpdated: number
  commits: number
  skipped: number
  needsSignIn: number
  changesInTheWay: number
  /** Branches left alone or in trouble, for a single-project summary. */
  branchesNeedingLook: number
}

export function updateTotals(repos: RepoUpdate[]): UpdateTotals {
  const totals: UpdateTotals = {
    projects: repos.length,
    errors: 0,
    warnings: 0,
    updatedProjects: 0,
    branchesUpdated: 0,
    commits: 0,
    skipped: 0,
    needsSignIn: 0,
    changesInTheWay: 0,
    branchesNeedingLook: 0,
  }
  for (const repo of repos) {
    if (repo.skipped) {
      totals.skipped += 1
      continue
    }
    if (repo.level === 'error') totals.errors += 1
    if (repo.level === 'warning') totals.warnings += 1
    const updated = repo.branches.filter((b) => b.kind === 'updated').length
    if (updated > 0) totals.updatedProjects += 1
    totals.branchesUpdated += updated
    totals.commits += repo.commits_received
    if (repo.needs_sign_in) totals.needsSignIn += 1
    if (repo.changes_in_the_way) totals.changesInTheWay += 1
    totals.branchesNeedingLook += repo.branches.filter((b) => {
      const tone = branchUpdateTone(b)
      return tone === 'warning' || tone === 'error'
    }).length
  }
  return totals
}

/** The one-line headline for a finished run. */
export function updateHeadline(totals: UpdateTotals): string {
  if (totals.branchesUpdated === 0) {
    return totals.errors + totals.warnings === 0 ? 'Everything is already up to date' : 'Nothing new was brought in'
  }
  const where = totals.projects > 1 ? ` in ${plural(totals.updatedProjects, 'project')}` : ''
  return `Updated ${plural(totals.branchesUpdated, 'branch', 'branches')}${where}`
}

/** The second line for a finished run: new commits, then anything to look at. */
export function updateDetail(totals: UpdateTotals, cancelled: boolean, scope: 'all' | 'repo' = 'all'): string {
  const parts: string[] = []
  if (totals.commits > 0) parts.push(plural(totals.commits, 'new commit'))
  const attention = scope === 'repo' ? totals.branchesNeedingLook : totals.errors + totals.warnings
  const noun = scope === 'repo' ? plural(attention, 'branch', 'branches') : plural(attention, 'project')
  if (attention > 0) parts.push(`${noun} need${attention === 1 ? 's' : ''} a look`)
  if (cancelled && totals.skipped > 0) parts.push(`stopped before ${plural(totals.skipped, 'project')}`)
  return parts.join(' · ')
}

export type ResultSection = 'error' | 'warning' | 'updated' | 'unchanged' | 'skipped'

const RANK: Record<RepoUpdateLevel, number> = { error: 0, warning: 1, updated: 2, unchanged: 3 }

/** Errors first, then warnings, then what moved, then everything else. */
export function sortRepoUpdates(repos: RepoUpdate[]): RepoUpdate[] {
  return [...repos].sort(
    (a, b) =>
      RANK[a.level] - RANK[b.level] ||
      Number(a.skipped) - Number(b.skipped) ||
      a.name.toLowerCase().localeCompare(b.name.toLowerCase()),
  )
}

export function sectionOf(repo: RepoUpdate): ResultSection {
  return repo.skipped ? 'skipped' : repo.level
}

/** Group sorted results for display, dropping empty groups. */
export function groupRepoUpdates(repos: RepoUpdate[]): { section: ResultSection; repos: RepoUpdate[] }[] {
  const order: ResultSection[] = ['error', 'warning', 'updated', 'unchanged', 'skipped']
  const sorted = sortRepoUpdates(repos)
  return order
    .map((section) => ({ section, repos: sorted.filter((r) => sectionOf(r) === section) }))
    .filter((group) => group.repos.length > 0)
}

/**
 * Fold a retry's results into the report it was started from, so trying a few
 * projects again does not throw away what the rest of the run found.
 */
export function mergeRepoUpdates(previous: RepoUpdate[], retry: RepoUpdate[]): RepoUpdate[] {
  const byPath = new Map(previous.map((r) => [pathKey(r.path), r]))
  for (const repo of retry) byPath.set(pathKey(repo.path), repo)
  return sortRepoUpdates([...byPath.values()])
}
