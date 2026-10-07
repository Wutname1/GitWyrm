/**
 * "@" mentions of project files in the Agent Desk message box.
 *
 * Typing "@" after a space (or at the start) opens a list of the project's
 * files; picking one writes "@path" into the message, the form every agent
 * tool already reads as "look at this file".
 */

export interface FileMentionQuery {
  /** Index of the "@" in the draft. */
  start: number
  /** What follows the "@", up to the caret. */
  query: string
  matches: string[]
}

export const FILE_MENTION_LIMIT = 50

/** The open "@" list for this draft and caret, or `null`. */
export function matchFileMention(files: readonly string[], draft: string, caret: number = draft.length): FileMentionQuery | null {
  const before = draft.slice(0, caret)
  const at = before.lastIndexOf('@')
  if (at < 0) return null
  // An "@" inside a word is an email address or a handle, not a mention.
  if (at > 0 && !/\s/.test(before[at - 1])) return null
  const query = before.slice(at + 1)
  if (/\s/.test(query)) return null

  const q = query.toLowerCase()
  const byName: string[] = []
  const byPath: string[] = []
  for (const file of files) {
    const lower = file.toLowerCase()
    const name = lower.slice(lower.lastIndexOf('/') + 1)
    if (q === '' || name.startsWith(q)) byName.push(file)
    else if (lower.includes(q)) byPath.push(file)
    if (byName.length >= FILE_MENTION_LIMIT) break
  }
  const shortFirst = (a: string, b: string) => a.length - b.length || a.localeCompare(b)
  byName.sort(shortFirst)
  byPath.sort(shortFirst)
  const matches = [...byName, ...byPath].slice(0, FILE_MENTION_LIMIT)
  return matches.length > 0 ? { start: at, query, matches } : null
}

/** The draft with the "@query" being typed replaced by "@path ". */
export function applyFileMention(draft: string, mention: FileMentionQuery, path: string, caret: number): string {
  return `${draft.slice(0, mention.start)}@${path} ${draft.slice(caret).replace(/^ /, '')}`
}

/** A path's last segment, for a chip label. */
export function fileLabel(path: string): string {
  const trimmed = path.replace(/[\\/]+$/, '')
  return trimmed.slice(Math.max(trimmed.lastIndexOf('/'), trimmed.lastIndexOf('\\')) + 1) || path
}
