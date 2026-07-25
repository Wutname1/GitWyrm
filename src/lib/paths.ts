/** Normalizes a Windows path: trims, converts / to \, collapses repeats, strips trailing slashes (keeps drive roots like "C:\"). */
export function normalizePath(input: string): string {
  let p = input.trim().replace(/\//g, '\\').replace(/\\{2,}/g, '\\')
  // Strip trailing slashes, but keep "C:\" intact.
  p = p.replace(/\\+$/, '')
  if (/^[a-zA-Z]:$/.test(p)) p += '\\'
  return p
}

/**
 * Canonical form for comparing two Windows paths: normalized and lowercased,
 * because Windows paths are case-insensitive and "C:\Code\Repo" and
 * "c:/code/repo/" name the same folder.
 */
export function pathKey(path: string): string {
  return normalizePath(path).toLowerCase()
}

/** Whether two paths name the same folder, ignoring case and slash style. */
export function samePath(left: string, right: string): boolean {
  return pathKey(left) === pathKey(right)
}

/** Joins a base directory and a child name with exactly one backslash. */
export function joinPath(base: string, child: string): string {
  const b = normalizePath(base)
  return b.endsWith('\\') ? `${b}${child}` : `${b}\\${child}`
}
