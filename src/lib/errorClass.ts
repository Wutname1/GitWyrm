/**
 * Alert classification.
 *
 * Backend commands return raw error strings -- often straight from git2, e.g.
 * "git error: cannot stash changes - there is nothing to stash.; class=Stash
 * (19); code=NotFound (-3)". Showing that verbatim as a red error toast is wrong
 * twice over: the phrasing is developer jargon, and some of these "errors" are
 * benign no-ops that should never have alarmed the user in the first place.
 *
 * This module maps a raw message to a severity and a plain-language line. Call
 * sites pick the toast style from the severity instead of assuming `error`.
 */

export type Severity = 'error' | 'warning' | 'info'

export interface ClassifiedError {
  severity: Severity
  /** Plain-language message shown to the user. */
  message: string
  /** The original backend string, always kept for the log. */
  raw: string
}

interface Rule {
  /** Matches against the lowercased raw message. */
  match: (raw: string) => boolean
  severity: Severity
  message: string
}

/**
 * Ordered, most-specific first. The git2 `class=`/`code=` tail is the stable
 * part of these strings, so match on that where possible rather than prose that
 * may change between libgit2 versions.
 */
const RULES: Rule[] = [
  {
    // "nothing to stash" -- clean working tree. Not a failure.
    match: (r) => r.includes('nothing to stash') || r.includes('class=stash') && r.includes('code=notfound'),
    severity: 'info',
    message: 'Nothing to stash -- your working tree is already clean.',
  },
  {
    match: (r) => r.includes('your local changes conflict'),
    severity: 'warning',
    message: 'Your local changes conflict with that branch. Commit, stash, or discard them first.',
  },
  {
    // Server refused the update because the branch is protected -- most force
    // pushes to a shared main branch hit this. Nothing local will fix it.
    match: (r) =>
      r.includes('protected branch') || r.includes('gh006') || r.includes('cannot force-push'),
    severity: 'warning',
    message:
      "The remote won't let you replace this branch - it's protected. Open a pull request instead, or ask a maintainer to allow the change.",
  },
  {
    // Non-fast-forward: the cloud moved on since you last fetched. A plain push
    // is refused; the user needs to get those changes first or force past them.
    match: (r) =>
      r.includes('stale info') ||
      r.includes('non-fast-forward') ||
      r.includes('fetch first') ||
      r.includes('[rejected]') ||
      r.includes('remote rejected'),
    severity: 'warning',
    message:
      "The cloud has changes yours doesn't, so it turned down the push. Get those changes first, or force push to replace them.",
  },
  {
    // Branch switch blocked purely by a moved submodule pointer.
    match: (r) => r.includes('submodule points to a different commit'),
    severity: 'warning',
    message:
      'A submodule points to a different commit than this branch expects. Commit the submodule change or reset the submodule first.',
  },
  {
    match: (r) => r.includes('submodule'),
    severity: 'error',
    message: "Couldn't update the submodule. Check that it's set up and try again.",
  },
  {
    match: (r) => r.includes('code=conflict') || r.includes('merge conflict'),
    severity: 'warning',
    message: 'That ran into a conflict. Check the changed files and resolve the markers.',
  },
  {
    match: (r) =>
      r.includes('authentication') ||
      r.includes('credential') ||
      r.includes('401') ||
      r.includes('403'),
    severity: 'error',
    message: "Couldn't authenticate with the remote. Check your credentials and try again.",
  },
  {
    match: (r) =>
      r.includes('could not resolve host') ||
      r.includes('network') ||
      r.includes('timed out') ||
      r.includes('connection'),
    severity: 'error',
    message: "Couldn't reach the remote. Check your connection and try again.",
  },
]

/** Strip git2's "; class=... (N); code=... (N)" diagnostic tail for display. */
function stripGitDiagnostics(raw: string): string {
  return raw
    .replace(/;\s*class=[^;]*/gi, '')
    .replace(/;\s*code=[^;]*/gi, '')
    .replace(/^git error:\s*/i, '')
    .trim()
}

export function classifyError(e: unknown): ClassifiedError {
  const raw = e instanceof Error ? e.message : typeof e === 'string' ? e : String(e)
  const lower = raw.toLowerCase()

  for (const rule of RULES) {
    if (rule.match(lower)) {
      return { severity: rule.severity, message: rule.message, raw }
    }
  }

  // Unclassified: still an error, but show a cleaned-up message instead of the
  // git2 diagnostic tail. The raw string goes to the log regardless.
  const cleaned = stripGitDiagnostics(raw)
  return {
    severity: 'error',
    message: cleaned.length > 0 ? cleaned : 'Something went wrong.',
    raw,
  }
}
