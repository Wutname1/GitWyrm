/**
 * Convert a remote URL between its HTTPS and SSH forms.
 *
 * Hosts publish both shapes for the same repository, and which one you want
 * depends on how you authenticate -- a token in a credential helper wants
 * HTTPS, a key agent wants SSH. Swapping by hand means retyping the whole URL,
 * so the edit-remote form offers a one-click toggle instead.
 *
 * Only the two shapes below convert. Anything else (a local path, an unknown
 * scheme, an https URL with no repository path) returns null and the toggle
 * stays hidden rather than offering a swap that would produce a broken URL.
 *
 *   https://host/path(.git)     <->  git@host:path.git
 *   ssh://git@host(:port)/path  <->  https://host/path.git
 */

export type RemoteUrlKind = 'https' | 'ssh'

interface Split {
  kind: RemoteUrlKind
  /** Host without credentials or port. */
  host: string
  /** Repository path, slashes and any `.git` suffix trimmed. */
  path: string
  /** Explicit ssh port, preserved across a swap so a custom port survives. */
  port: string | null
}

function trimPath(path: string): string {
  const trimmed = path.replace(/^\/+|\/+$/g, '')
  // strip one suffix only: a repo really named `x.git.git` keeps one.
  return trimmed.replace(/\.git$/i, '').replace(/^\/+|\/+$/g, '')
}

/**
 * Break a remote URL into the parts a swap needs. Mirrors the shapes that
 * `git::remote_url` parses on the Rust side; kept in TS because the toggle acts
 * on the text the user is currently typing, not on a saved remote.
 */
function split(url: string): Split | null {
  const value = url.trim()
  if (!value) return null
  // A Windows drive path is a local remote, not a URL. Checked before the
  // scp branch, which would otherwise read `C` as the host.
  if (/^[a-z]:[\\/]/i.test(value)) return null
  if (value.startsWith('/') || value.startsWith('.') || value.startsWith('file://')) return null

  const schemeMatch = value.match(/^([a-z][a-z0-9+.-]*):\/\/(.+)$/i)
  if (schemeMatch) {
    const scheme = schemeMatch[1].toLowerCase()
    if (scheme !== 'https' && scheme !== 'http' && scheme !== 'ssh') return null
    const slash = schemeMatch[2].indexOf('/')
    if (slash === -1) return null
    const authority = schemeMatch[2].slice(0, slash)
    const path = trimPath(schemeMatch[2].slice(slash + 1))
    if (!path) return null

    const at = authority.lastIndexOf('@')
    const hostPort = at === -1 ? authority : authority.slice(at + 1)
    if (hostPort.startsWith('[')) return null // IPv6 literal; leave it alone
    const colon = hostPort.indexOf(':')
    const host = colon === -1 ? hostPort : hostPort.slice(0, colon)
    const port = colon === -1 ? null : hostPort.slice(colon + 1)
    if (!host) return null

    return { kind: scheme === 'ssh' ? 'ssh' : 'https', host, path, port }
  }

  // scp-like `user@host:path`
  const scp = value.match(/^(?:([^@/]+)@)?([^:/]+):(.+)$/)
  if (!scp) return null
  const host = scp[2]
  if (host.length < 2) return null // single character is a drive letter
  const path = trimPath(scp[3])
  if (!path) return null
  return { kind: 'ssh', host, path, port: null }
}

/** Which form a remote URL is in, or null when it is neither. */
export function remoteUrlKind(url: string): RemoteUrlKind | null {
  return split(url)?.kind ?? null
}

/**
 * The same repository expressed in the other form, or null when the URL is not
 * one we can convert.
 *
 * An ssh URL with an explicit port or a non-`git` user keeps the `ssh://` shape
 * on the way back, because the scp-like form cannot carry a port.
 */
export function swapRemoteUrl(url: string): string | null {
  const parts = split(url)
  if (!parts) return null

  if (parts.kind === 'https') {
    // An https `user@` is an account name for the credential helper, not an ssh
    // login -- carrying it over would produce a URL that cannot authenticate.
    if (parts.port) return `ssh://git@${parts.host}:${parts.port}/${parts.path}.git`
    return `git@${parts.host}:${parts.path}.git`
  }

  return `https://${parts.host}/${parts.path}.git`
}
