/** Normalizes a Windows path: trims, converts / to \, collapses repeats, strips trailing slashes (keeps drive roots like "C:\"). */
export function normalizePath(input: string): string {
  let p = input.trim().replace(/\//g, '\\').replace(/\\{2,}/g, '\\')
  // Strip trailing slashes, but keep "C:\" intact.
  p = p.replace(/\\+$/, '')
  if (/^[a-zA-Z]:$/.test(p)) p += '\\'
  return p
}

/** Joins a base directory and a child name with exactly one backslash. */
export function joinPath(base: string, child: string): string {
  const b = normalizePath(base)
  return b.endsWith('\\') ? `${b}${child}` : `${b}\\${child}`
}
