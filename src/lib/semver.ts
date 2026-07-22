// Lightweight semver parsing for the tag dialog's quick-pick buttons. We only
// care about plain MAJOR.MINOR.PATCH tags (with an optional leading "v"); tags
// carrying pre-release or build metadata are treated as non-semver so we don't
// guess wrong about what "next" means for them.

export type SemVer = {
  major: number
  minor: number
  patch: number
  // Whether the original tag started with a "v" (e.g. "v1.2.3"). Kept so we can
  // suggest new tags in the same style the user already uses.
  hasV: boolean
}

const SEMVER_RE = /^v?(\d+)\.(\d+)\.(\d+)$/

export function parseSemver(name: string): SemVer | null {
  const match = SEMVER_RE.exec(name.trim())
  if (!match) return null
  return {
    major: Number(match[1]),
    minor: Number(match[2]),
    patch: Number(match[3]),
    hasV: name.trim().startsWith('v'),
  }
}

export function compareSemver(a: SemVer, b: SemVer): number {
  return a.major - b.major || a.minor - b.minor || a.patch - b.patch
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

export type Bump = 'major' | 'minor' | 'patch'

export function bumpSemver(v: SemVer, bump: Bump): SemVer {
  if (bump === 'major') return { ...v, major: v.major + 1, minor: 0, patch: 0 }
  if (bump === 'minor') return { ...v, minor: v.minor + 1, patch: 0 }
  return { ...v, patch: v.patch + 1 }
}

export function formatSemver(v: SemVer): string {
  const core = `${v.major}.${v.minor}.${v.patch}`
  return v.hasV ? `v${core}` : core
}
