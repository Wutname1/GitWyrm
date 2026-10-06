/**
 * Wording for what Mehen, the dependency checker, found in a repository.
 *
 * GitWyrm never checks packages itself. It shows Mehen's last answer, so every
 * sentence here says when that answer is from and never claims more than it.
 */
import type { MehenAttention, MehenFlagged, MehenProblem, MehenPushNote, MehenRepoStatus } from './bindings'
import { formatRelativeTime } from './gitDisplay'
import { pathKey } from './paths'

/** After this long, Mehen's answer is shown as possibly out of date. */
export const MEHEN_STALE_SECONDS = 7 * 24 * 60 * 60

export function isMehenStale(checkedAt: number | null, now = Date.now()): boolean {
  return checkedAt == null || now / 1000 - checkedAt > MEHEN_STALE_SECONDS
}

export function unsafePackages(n: number): string {
  return n === 1 ? '1 package has a known security problem' : `${n} packages have known security problems`
}

/** "checked 3h ago", or "checked a while ago" when the time is unknown. */
export function checkedWhen(checkedAt: number | null, now = Date.now()): string {
  return checkedAt == null ? 'checked a while ago' : `checked ${formatRelativeTime(checkedAt, now)}`
}

function fileList(files: string[]): string {
  const names = files.map((f) => f.split('/').pop() ?? f)
  const unique = [...new Set(names)]
  if (unique.length <= 2) return unique.join(' and ')
  return `${unique.slice(0, 2).join(', ')} and ${unique.length - 2} more`
}

/** One line for the Push button's hover text, before anything is sent. */
export function pushNoteHint(note: MehenPushNote, now = Date.now()): string {
  const found = `Mehen found ${note.fixable === 1 ? '1 package' : `${note.fixable} packages`} with known security problems here (${checkedWhen(note.checked_at, now)})`
  return `These commits change ${fileList(note.files)}. ${found}${note.seen_by_mehen ? '.' : ', before these changes.'}`
}

/** The message shown after a push that sent dependency changes. */
export function pushNoteToast(note: MehenPushNote): { title: string; description: string } {
  return {
    title: 'You sent changes to your packages',
    description: note.seen_by_mehen
      ? `Mehen says ${unsafePackages(note.fixable)} in this project.`
      : `Before these changes, Mehen found ${note.fixable === 1 ? '1 package' : `${note.fixable} packages`} with known security problems here. Check again in Mehen to be sure.`,
  }
}

/** One fix, stable across checks: the repository, the package and the version that fixes it. */
export function fixKey(repoPath: string, problem: MehenProblem): string {
  return `${pathKey(repoPath)}|${problem.ecosystem}:${problem.name}@${problem.fixed_in}`
}

export function allFixKeys(repos: MehenRepoStatus[]): string[] {
  return repos.flatMap((r) => r.problems.map((p) => fixKey(r.path, p)))
}

/**
 * Open repositories with fixes that were not there last time, and how many.
 * Repositories that are not open stay quiet: their fixes are remembered as
 * seen, so opening one later does not announce old news.
 */
export function newFixes(repos: MehenRepoStatus[], openPaths: string[], seen: ReadonlySet<string>): { repo: MehenRepoStatus; count: number }[] {
  return repos
    .filter((r) => openPaths.some((p) => pathKey(p) === pathKey(r.path)))
    .map((repo) => ({ repo, count: repo.problems.filter((p) => !seen.has(fixKey(repo.path, p))).length }))
    .filter(({ count }) => count > 0)
}

/**
 * What the Mehen badge on a repository tab counts, from most to least urgent.
 * Each level includes every level above it. Security problems only count when
 * they have a fix.
 */
export const MEHEN_TAB_LEVELS = [
  { id: 'off', label: 'Nothing' },
  { id: 'critical', label: 'Critical security fixes' },
  { id: 'high', label: 'High and critical security fixes' },
  { id: 'moderate', label: 'Medium and higher security fixes' },
  { id: 'security', label: 'All security fixes' },
  { id: 'major', label: 'Security fixes and major updates' },
  { id: 'minor', label: 'Security fixes, major and minor updates' },
  { id: 'all', label: 'Every package with an update' },
] as const

export type MehenTabLevel = (typeof MEHEN_TAB_LEVELS)[number]['id']

export const DEFAULT_MEHEN_TAB_LEVEL: MehenTabLevel = 'security'

export function parseMehenTabLevel(value: string | null | undefined): MehenTabLevel {
  return MEHEN_TAB_LEVELS.find((l) => l.id === value)?.id ?? DEFAULT_MEHEN_TAB_LEVEL
}

/** Attention levels in order, most urgent first, as Mehen counts them. */
const ATTENTION_ORDER = ['critical', 'high', 'moderate', 'low', 'major', 'minor', 'patch'] as const
/** The last attention level each tab level includes. */
const LEVEL_REACH: Record<Exclude<MehenTabLevel, 'off'>, (typeof ATTENTION_ORDER)[number]> = {
  critical: 'critical',
  high: 'high',
  moderate: 'moderate',
  security: 'low',
  major: 'major',
  minor: 'minor',
  all: 'patch',
}

/**
 * The tab badge for one repository: how many packages it counts, and whether
 * any of them is a security fix (which decides its colour). Zero hides it.
 */
export function mehenTabBadge(attention: MehenAttention, level: MehenTabLevel): { count: number; security: number } {
  if (level === 'off') return { count: 0, security: 0 }
  const reach = ATTENTION_ORDER.indexOf(LEVEL_REACH[level])
  const included = ATTENTION_ORDER.slice(0, reach + 1)
  const count = included.reduce((sum, key) => sum + attention[key], 0)
  const security = included.filter((key) => ATTENTION_ORDER.indexOf(key) <= ATTENTION_ORDER.indexOf('low')).reduce((sum, key) => sum + attention[key], 0)
  return { count, security }
}

export type MehenLevel = (typeof ATTENTION_ORDER)[number]

export const MEHEN_LEVEL_LABEL: Record<MehenLevel, string> = {
  critical: 'Critical',
  high: 'High',
  moderate: 'Medium',
  low: 'Low',
  major: 'Major',
  minor: 'Minor',
  patch: 'Patch',
}

/** A security fix, as opposed to a plain update. */
export function isSecurityLevel(level: string): boolean {
  const at = ATTENTION_ORDER.indexOf(level as MehenLevel)
  return at >= 0 && at <= ATTENTION_ORDER.indexOf('low')
}

/** The packages Mehen flags that the chosen level includes, most urgent first. */
export function flaggedAt(flagged: MehenFlagged[], level: MehenTabLevel): MehenFlagged[] {
  if (level === 'off') return []
  const reach = ATTENTION_ORDER.indexOf(LEVEL_REACH[level])
  return flagged.filter((f) => {
    const at = ATTENTION_ORDER.indexOf(f.level as MehenLevel)
    return at >= 0 && at <= reach
  })
}

/** One wording for the tab badge, the status bar and the sidebar. */
export function mehenBadgeLabel({ count, security }: { count: number; security: number }): string {
  const packages = count === 1 ? '1 package' : `${count} packages`
  if (security === count) return `${packages} with a security fix waiting`
  if (security === 0) return `${packages} to update`
  return `${packages} to update, ${security === 1 ? '1 of them a security fix' : `${security} of them security fixes`}`
}
