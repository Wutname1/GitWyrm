/**
 * Wording for what Mehen, the dependency checker, found in a repository.
 *
 * GitWyrm never checks packages itself. It shows Mehen's last answer, so every
 * sentence here says when that answer is from and never claims more than it.
 */
import type { MehenPushNote } from './bindings'
import { formatRelativeTime } from './gitDisplay'

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
