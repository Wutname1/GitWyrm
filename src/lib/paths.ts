/** Whether a path uses Windows drive/UNC syntax rather than POSIX syntax. */
function isWindowsPath(path: string): boolean {
  return /^[a-zA-Z]:[\\/]/.test(path) || /^\\\\/.test(path)
}

/**
 * Normalizes a native path without changing its platform.
 *
 * Older builds converted every slash to a backslash. On Linux that turned
 * `/home/me/repo` into `\home\me\repo`, so a single leading backslash is
 * repaired as a POSIX root while real Windows drive and UNC paths stay intact.
 */
export function normalizePath(input: string): string {
  const raw = input.trim()
  if (!raw) return raw

  if (isWindowsPath(raw)) {
    const unc = raw.startsWith('\\\\')
    let path = raw.replace(/[\\/]+/g, '\\')
    if (unc) path = `\\${path}`
    path = path.replace(/\\+$/, '')
    if (/^[a-zA-Z]:$/.test(path)) path += '\\'
    return path
  }

  // A single leading backslash is the shape written by the old Linux bug.
  let path = raw.startsWith('\\') ? `/${raw.slice(1)}` : raw
  path = path.replace(/\\/g, '/').replace(/\/{2,}/g, '/')
  if (path !== '/') path = path.replace(/\/+$/, '')
  return path
}

/**
 * Canonical form for comparing paths. Windows is case-insensitive; Linux is
 * not, so lowercasing a POSIX path could merge two different repositories.
 */
export function pathKey(path: string): string {
  const normalized = normalizePath(path)
  return isWindowsPath(normalized) ? normalized.toLowerCase() : normalized
}

/** Whether two paths name the same native folder. */
export function samePath(left: string, right: string): boolean {
  return pathKey(left) === pathKey(right)
}

/** The last folder or file name from a Windows or POSIX path. */
export function pathName(input: string): string {
  const path = normalizePath(input)
  return path.split(/[\\/]/).filter(Boolean).at(-1) ?? path
}

/** Joins a base directory and child using the base path's native separator. */
export function joinPath(base: string, child: string): string {
  const b = normalizePath(base)
  const separator = isWindowsPath(b) ? '\\' : '/'
  return normalizePath(`${b}${b.endsWith(separator) ? '' : separator}${child}`)
}
