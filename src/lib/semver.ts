// Lightweight semver parsing for the tag dialog's quick-pick buttons. Handles
// plain MAJOR.MINOR.PATCH tags (with an optional leading "v") and pre-release
// tags like "1.2.3-beta.4" or "1.2.3-rc1", because a repo tagging every build as
// a beta is exactly the repo that most wants a next-version suggestion. Build
// metadata ("+sha") is still treated as non-semver: it does not order, so there
// is nothing sensible to bump.

export type SemVer = {
  major: number
  minor: number
  patch: number
  /**
   * The pre-release identifiers, already split on ".". Empty for a stable
   * release. `["beta", 4]` for "1.2.3-beta.4"; numeric identifiers are kept as
   * numbers because semver compares those numerically.
   */
  pre: (string | number)[]
  // Whether the original tag started with a "v" (e.g. "v1.2.3"). Kept so we can
  // suggest new tags in the same style the user already uses.
  hasV: boolean
}

// The pre-release group is the semver spec's: dot-separated alphanumeric and
// hyphen identifiers. Build metadata is deliberately not matched.
const SEMVER_RE = /^v?(\d+)\.(\d+)\.(\d+)(?:-([0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*))?$/

/** Numeric identifiers compare as numbers; everything else stays a string. */
function parseIdentifier(raw: string): string | number {
  return /^\d+$/.test(raw) ? Number(raw) : raw
}

export function parseSemver(name: string): SemVer | null {
  const trimmed = name.trim()
  const match = SEMVER_RE.exec(trimmed)
  if (!match) return null
  return {
    major: Number(match[1]),
    minor: Number(match[2]),
    patch: Number(match[3]),
    pre: match[4] ? match[4].split('.').map(parseIdentifier) : [],
    hasV: trimmed.startsWith('v'),
  }
}

/**
 * Compare pre-release identifier lists per the semver spec: numbers sort below
 * strings, and a list that runs out first is the lower version ("beta" <
 * "beta.1"). Callers handle the "no pre-release at all" case first.
 */
function comparePre(a: (string | number)[], b: (string | number)[]): number {
  const len = Math.min(a.length, b.length)
  for (let i = 0; i < len; i++) {
    const x = a[i]
    const y = b[i]
    if (x === y) continue
    const xNum = typeof x === 'number'
    const yNum = typeof y === 'number'
    if (xNum && yNum) return (x as number) - (y as number)
    // Numeric identifiers always have lower precedence than alphanumeric ones.
    if (xNum !== yNum) return xNum ? -1 : 1
    return (x as string) < (y as string) ? -1 : 1
  }
  return a.length - b.length
}

export function compareSemver(a: SemVer, b: SemVer): number {
  const core = a.major - b.major || a.minor - b.minor || a.patch - b.patch
  if (core !== 0) return core
  // A pre-release comes *before* the release it leads up to: 1.2.3-rc1 < 1.2.3.
  const aPre = a.pre.length > 0
  const bPre = b.pre.length > 0
  if (aPre !== bPre) return aPre ? -1 : 1
  if (!aPre) return 0
  return comparePre(a.pre, b.pre)
}

/**
 * Given every existing tag name, decide whether this repo tags with semver and,
 * if so, return the highest version. Returns null when fewer than half the tags
 * parse as semver -- that repo isn't using semver and quick-picks would be noise.
 */
export function highestSemver(names: string[]): SemVer | null {
  if (names.length === 0) return null
  const parsed = names.map(parseSemver).filter((v): v is SemVer => v !== null)
  if (parsed.length === 0) return null
  if (parsed.length * 2 < names.length) return null
  return parsed.reduce((best, v) => (compareSemver(v, best) > 0 ? v : best))
}

/**
 * `prerelease` continues the current pre-release line (beta.4 -> beta.5), and
 * `release` drops the suffix to ship the version the line was leading up to.
 * Both only apply when the highest tag is itself a pre-release.
 */
export type Bump = 'major' | 'minor' | 'patch' | 'prerelease' | 'release'

/**
 * Bump the trailing numeric identifier of a pre-release ("beta.4" -> "beta.5",
 * "rc1" -> "rc2"). Returns null when there is no number to advance, so the
 * caller can leave the suggestion out rather than invent a shape.
 */
function bumpPre(pre: (string | number)[]): (string | number)[] | null {
  // The common "beta.4" shape: a trailing numeric identifier.
  const last = pre[pre.length - 1]
  if (typeof last === 'number') {
    return [...pre.slice(0, -1), last + 1]
  }
  // The "rc1" shape: digits glued to the end of a single identifier.
  if (typeof last === 'string') {
    const match = /^(.*?)(\d+)$/.exec(last)
    if (match) {
      return [...pre.slice(0, -1), `${match[1]}${Number(match[2]) + 1}`]
    }
  }
  return null
}

/**
 * Returns null when the bump does not apply to this version -- asking for
 * `prerelease` or `release` on a stable tag, or for a pre-release bump on a
 * suffix with no number in it.
 */
export function bumpSemver(v: SemVer, bump: Bump): SemVer | null {
  const isPre = v.pre.length > 0
  if (bump === 'release') {
    return isPre ? { ...v, pre: [] } : null
  }
  if (bump === 'prerelease') {
    if (!isPre) return null
    const next = bumpPre(v.pre)
    return next ? { ...v, pre: next } : null
  }
  // A pre-release is already "on the way to" its own version, so bumping the
  // core from 1.2.3-beta.4 starts from 1.2.3, not from 1.2.4.
  const base = { ...v, pre: [] }
  if (bump === 'major') {
    if (isPre && v.minor === 0 && v.patch === 0) return base
    return { ...base, major: v.major + 1, minor: 0, patch: 0 }
  }
  if (bump === 'minor') {
    if (isPre && v.patch === 0) return base
    return { ...base, minor: v.minor + 1, patch: 0 }
  }
  if (isPre) return base
  return { ...base, patch: v.patch + 1 }
}

export function formatSemver(v: SemVer): string {
  const core = `${v.major}.${v.minor}.${v.patch}`
  const pre = v.pre.length > 0 ? `-${v.pre.join('.')}` : ''
  return v.hasV ? `v${core}${pre}` : `${core}${pre}`
}
