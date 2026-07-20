/**
 * Rules git enforces on branch and tag names, checked before we ask git so the
 * user gets a sentence instead of a raw libgit2 error.
 *
 * Verified against `git2::Reference::is_valid_name` (see the ref-name test in
 * src-tauri/tests/mutations.rs): whitespace and the special characters
 * `~^:?*[\`, a `..` sequence, `@{`, a leading or trailing slash, a doubled
 * slash, a leading or trailing dot, and a trailing `.lock`.
 *
 * A lone `@` is NOT included -- git accepts `refs/heads/@`, and rejecting it
 * here would refuse a name git allows. This is not the complete rule set (git
 * also rejects control characters), so the server checks too; this exists to
 * explain the common mistakes, not to be the boundary.
 */
const INVALID = /[\s~^:?*[\\\]]|\.\.|@\{|^\/|\/$|\/\/|^\.|\.$|\.lock$/

export type RefKind = 'branch' | 'tag'

/**
 * Why `name` is not usable, or null when it is fine.
 *
 * `existing` are the names already taken, so a collision reads as a collision
 * rather than as a git error after the fact.
 */
export function refNameError(
  name: string,
  existing: string[],
  kind: RefKind
): string | null {
  const trimmed = name.trim()
  if (!trimmed) return null // empty is "not ready", not "wrong" -- no error shown
  if (existing.includes(trimmed)) return `There's already a ${kind} called ${trimmed}.`
  if (INVALID.test(trimmed))
    return "That name has characters git won't accept. Try letters, numbers, dashes and slashes."
  return null
}

/** Whether the name is complete and legal, i.e. safe to submit. */
export function isValidRefName(name: string, existing: string[], kind: RefKind): boolean {
  const trimmed = name.trim()
  return trimmed !== '' && refNameError(trimmed, existing, kind) === null
}
