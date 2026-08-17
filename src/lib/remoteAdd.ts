/**
 * Helpers for the "add a remote" form: pick a name when the user gives none,
 * and turn whatever address they pasted into one git can actually fetch from.
 *
 * People add a remote by copying the page they are looking at -- a branch view,
 * a pull request, the address bar of the repository home page. Those are all
 * the same repository, so the form accepts them and cleans them up instead of
 * asking the user to know which shape git wants.
 *
 * Everything here is pure string work so it can be unit tested; the modal owns
 * the wiring.
 */

import { detectProvider, type RemoteProvider } from '@/lib/remoteProvider'

/** The name used when the user leaves the name box blank and it is free. */
export const DEFAULT_REMOTE_NAME = 'origin'

interface ParsedRemote {
  /** How the address was written, so it can be rebuilt the same way. */
  shape: 'scheme' | 'scp'
  scheme: string
  /** Anything before `@` in the authority (credential or ssh user). */
  user: string | null
  host: string
  /** Port including the leading colon, or '' when there was none. */
  port: string
  /** Path with no leading or trailing slashes, query, or fragment. */
  path: string
  /** True when the address already ended in `.git`. */
  hadGitSuffix: boolean
}

/** Local paths are remotes too, and none of the URL cleanup applies to them. */
function isLocalPath(value: string): boolean {
  return (
    /^[a-z]:[\\/]/i.test(value) ||
    value.startsWith('/') ||
    value.startsWith('.') ||
    value.startsWith('\\\\') ||
    value.startsWith('file://')
  )
}

function stripQuery(path: string): string {
  return path.split(/[?#]/)[0]
}

function trimSlashes(path: string): string {
  return path.replace(/^\/+|\/+$/g, '')
}

function parse(value: string): ParsedRemote | null {
  const schemeMatch = value.match(/^([a-z][a-z0-9+.-]*):\/\/(.+)$/i)
  if (schemeMatch) {
    const scheme = schemeMatch[1].toLowerCase()
    const rest = schemeMatch[2]
    const slash = rest.indexOf('/')
    if (slash === -1) return null
    const authority = rest.slice(0, slash)
    const at = authority.lastIndexOf('@')
    const user = at === -1 ? null : authority.slice(0, at)
    const hostPort = at === -1 ? authority : authority.slice(at + 1)
    if (hostPort.startsWith('[')) return null // IPv6 literal; leave it alone
    const colon = hostPort.indexOf(':')
    const host = colon === -1 ? hostPort : hostPort.slice(0, colon)
    const port = colon === -1 ? '' : hostPort.slice(colon)
    const path = trimSlashes(stripQuery(rest.slice(slash + 1)))
    if (!host || !path) return null
    return { shape: 'scheme', scheme, user, host, port, path, ...gitSuffix(path) }
  }

  // scp-like `user@host:path`
  const scp = value.match(/^(?:([^@/]+)@)?([^:/]+):(.+)$/)
  if (scp) {
    const host = scp[2]
    if (host.length < 2) return null // a single character is a drive letter
    const path = trimSlashes(stripQuery(scp[3]))
    if (!path) return null
    return { shape: 'scp', scheme: 'ssh', user: scp[1] ?? null, host, port: '', path, ...gitSuffix(path) }
  }

  // A bare `host/owner/repo` paste, which browsers show without the scheme.
  const bare = value.match(/^([a-z0-9.-]+\.[a-z]{2,})(\/.+)$/i)
  if (bare) {
    const path = trimSlashes(stripQuery(bare[2]))
    if (!path) return null
    return { shape: 'scheme', scheme: 'https', user: null, host: bare[1], port: '', path, ...gitSuffix(path) }
  }

  return null
}

function gitSuffix(path: string): { hadGitSuffix: boolean } {
  return { hadGitSuffix: /\.git$/i.test(path) }
}

/**
 * Cut a web page path back to the repository itself.
 *
 * Hosts hang their whole UI off the repository path -- `/tree/main`,
 * `/pull/12`, `/-/blob/...` -- so the repository is a fixed number of leading
 * segments for GitHub and Bitbucket, and everything before the `/-/` marker on
 * GitLab. Hosts we don't recognize keep their path untouched, because we can't
 * tell a UI route from a repository that genuinely lives three levels deep.
 */
function repositoryPath(path: string, provider: RemoteProvider): string {
  const withoutGit = path.replace(/\.git$/i, '')
  const segments = trimSlashes(withoutGit).split('/').filter(Boolean)
  if (segments.length === 0) return ''

  switch (provider) {
    // These three need an owner and a repository, so a shorter path is a URL
    // still being typed: leave it alone rather than "fixing" half an address.
    case 'github':
    case 'bitbucket':
      return segments.length < 2 ? '' : segments.slice(0, 2).join('/')
    case 'gitlab': {
      const marker = segments.indexOf('-')
      const repo = marker === -1 ? segments : segments.slice(0, marker)
      return repo.length < 2 ? '' : repo.join('/')
    }
    default:
      return segments.join('/')
  }
}

/** Hosts whose clone address canonically ends in `.git`. */
function wantsGitSuffix(provider: RemoteProvider): boolean {
  return provider === 'github' || provider === 'gitlab' || provider === 'bitbucket'
}

/**
 * The address git should actually store for whatever the user pasted.
 *
 * Handles a copied `git clone ...` command, surrounding quotes, a missing
 * `https://`, a page deep inside the host's UI, and a missing `.git`. Anything
 * that isn't recognizably a URL -- a local path, a half-typed address -- comes
 * back trimmed and otherwise untouched, so the field never fights the user
 * while they type.
 */
export function normalizeRemoteUrl(input: string): string {
  let value = input.trim().replace(/^["'`<]+|["'`>]+$/g, '').trim()
  // Pasting the whole command off a host's clone panel is common. Take its
  // first address-shaped word, so flags and their values (`--depth 1`) are
  // skipped along with the command itself.
  const command = value.match(/^git\s+clone\s+(.+)$/i)
  if (command) {
    const address = command[1].split(/\s+/).find((word) => !word.startsWith('-') && /[:/@]/.test(word))
    if (address) value = address
  }
  if (!value || isLocalPath(value)) return value

  const parts = parse(value)
  if (!parts) return value

  // Classify by host alone: the pasted shape may be one detectProvider can't
  // read (a bare `github.com/o/r`), while the host is always in hand here.
  const provider = detectProvider(`https://${parts.host}/${parts.path}`)

  const path = repositoryPath(parts.path, provider)
  if (!path) return value

  const suffix = wantsGitSuffix(provider) || parts.hadGitSuffix ? '.git' : ''

  if (parts.shape === 'scp') {
    return `${parts.user ?? 'git'}@${parts.host}:${path}${suffix}`
  }
  const credential = parts.user ? `${parts.user}@` : ''
  return `${parts.scheme}://${credential}${parts.host}${parts.port}/${path}${suffix}`
}

/** Git remote names allow letters, numbers, dots, dashes, and underscores. */
function sanitizeName(raw: string): string {
  return raw
    .toLowerCase()
    .replace(/[^a-z0-9._-]+/g, '-')
    .replace(/-{2,}/g, '-')
    .replace(/^[-.]+|[-.]+$/g, '')
}

/**
 * The account or group the repository belongs to, which reads better as a
 * second remote's name than `origin-2` does -- a fork of
 * `github.com/comcast-mgee/MGEEAnalytics` becomes `comcast-mgee`.
 */
function ownerFromUrl(url: string): string | null {
  const value = url.trim()
  if (!value || isLocalPath(value)) return null
  const parts = parse(value)
  if (!parts) return null
  const segments = trimSlashes(parts.path).split('/').filter(Boolean)
  // Bitbucket Server puts every repository under `/scm/`, which names nothing.
  const owner = segments[0]?.toLowerCase() === 'scm' ? segments[1] : segments[0]
  const name = owner ? sanitizeName(owner.replace(/\.git$/i, '')) : ''
  return name === '' ? null : name
}

/**
 * The name to use when the user adds a remote without typing one.
 *
 * `origin` is what git and every tutorial call the first remote, so it wins
 * whenever it is free. After that we fall back to the owner from the address,
 * then `upstream`, then a numbered `origin`, so the box always has a working
 * answer behind it and the user never has to invent a name.
 */
export function suggestRemoteName(url: string, taken: Iterable<string> = []): string {
  const used = new Set(taken)
  const candidates = [DEFAULT_REMOTE_NAME, ownerFromUrl(url), 'upstream']
  for (const candidate of candidates) {
    if (candidate && !used.has(candidate)) return candidate
  }
  for (let n = 2; ; n += 1) {
    const numbered = `${DEFAULT_REMOTE_NAME}-${n}`
    if (!used.has(numbered)) return numbered
  }
}

/**
 * A plain-language reason the typed name won't work, or null when it is fine.
 * Blank is fine: the form falls back to the suggested name.
 */
export function remoteNameError(name: string, taken: Iterable<string> = []): string | null {
  const value = name.trim()
  if (value === '') return null
  if (new Set(taken).has(value)) return 'That name is already used.'
  if (/\s/.test(value)) return "Names can't have spaces."
  if (!/^[A-Za-z0-9._-]+$/.test(value)) return 'Use letters, numbers, dashes, dots, or underscores.'
  if (value.startsWith('-') || value.startsWith('.')) return "Names can't start with a dash or dot."
  return null
}
