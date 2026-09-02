/**
 * Recognises a repository address typed into the picker's search box.
 *
 * Pasting a clone URL where you meant to search is the normal way people ask
 * "do I already have this?". The box answers that instead of returning nothing,
 * so this only has to decide whether what was typed is an address at all -- the
 * matching against local repositories happens in Rust, where a single parser
 * already knows every URL shape.
 *
 * Kept deliberately loose. A false positive costs one cheap lookup that finds
 * nothing; a false negative silently drops the whole feature.
 */

/** A repository address the search box recognised, ready to look up. */
export interface RepoUrlQuery {
  /** The address as typed, trimmed. */
  url: string
  /** `owner/repo` when the address has that shape, for display. */
  slug: string | null
  /** Host without credentials or port, e.g. `github.com`. */
  host: string
}

/** Strips a trailing `.git`, and any surrounding slashes. */
function cleanPath(path: string): string {
  const trimmed = path.replace(/^\/+|\/+$/g, '')
  const withoutGit = /\.git$/i.test(trimmed) ? trimmed.slice(0, -4) : trimmed
  return withoutGit.replace(/^\/+|\/+$/g, '')
}

/** Drops credentials and a port from an authority, lowercased. */
function cleanHost(authority: string): string {
  const host = authority.split('@').at(-1) ?? ''
  return host.split(':')[0]!.trim().toLowerCase()
}

/**
 * Reads a repository address out of search text, or null when it is an ordinary
 * search. Accepts the shapes a host will hand you: `https://`, `http://`,
 * `ssh://`, `git://`, and the scp-like `git@host:owner/repo`.
 */
export function parseRepoUrlQuery(text: string): RepoUrlQuery | null {
  const url = text.trim().replace(/[?#].*$/, '')
  if (!url || /\s/.test(url)) return null
  // A Windows drive path is a folder, not an address, and would otherwise read
  // as scp-like with `C` for a host.
  if (/^[a-z]:[\\/]/i.test(url)) return null
  if (url.startsWith('/') || url.startsWith('.') || url.startsWith('\\')) {
    return null
  }

  let authority: string
  let path: string
  const scheme = url.match(/^([a-z][a-z0-9+.-]*):\/\/(.+)$/i)
  if (scheme) {
    if (!['https', 'http', 'ssh', 'git'].includes(scheme[1]!.toLowerCase())) {
      return null
    }
    const rest = scheme[2]!
    const slash = rest.indexOf('/')
    if (slash < 0) return null
    authority = rest.slice(0, slash)
    path = rest.slice(slash + 1)
  } else {
    const colon = url.indexOf(':')
    if (colon < 2) return null
    authority = url.slice(0, colon)
    path = url.slice(colon + 1)
    if (authority.includes('/')) return null
  }

  const host = cleanHost(authority)
  const cleaned = cleanPath(path)
  if (!host || !host.includes('.') || !cleaned) return null

  const segments = cleaned.split('/')
  const slug = segments.length === 2 ? cleaned : null
  return { url, host, slug }
}
