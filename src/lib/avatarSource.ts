/**
 * Resolving a commit author's picture.
 *
 * A commit only records a name and an email, so the host account behind it is
 * usually a guess. The one exception is GitHub's no-reply address, which
 * embeds the account id ("1234+octocat@users.noreply.github.com") or, on older
 * commits, the login ("octocat@users.noreply.github.com"). Both map straight
 * to an avatar with no API call and no token, so they are tried first and the
 * result is exact.
 *
 * Everything else falls back to Gravatar. GitLab's /avatar endpoint is
 * deliberately not used: for anyone who is not a project member it just
 * returns a secure.gravatar.com identicon, so it costs an extra round-trip to
 * arrive at the image Gravatar already gives us directly.
 */

/** How long a resolution stays trusted before we look again. */
const TTL_MS = 7 * 24 * 60 * 60 * 1000
const STORAGE_KEY = 'gitwyrm.avatarCache.v1'
const MAX_ENTRIES = 2000

interface CacheEntry {
  /** Resolved image URL without its size parameter, or null for "no picture". */
  url: string | null
  at: number
}

type CacheMap = Record<string, CacheEntry>

let store: CacheMap | null = null
/** Coalesces the in-flight network probes so a scroll can't stampede. */
const inflight = new Map<string, Promise<string | null>>()

function load(): CacheMap {
  if (store) return store
  store = {}
  try {
    const raw = localStorage.getItem(STORAGE_KEY)
    if (raw) {
      const parsed: unknown = JSON.parse(raw)
      if (parsed && typeof parsed === 'object') store = parsed as CacheMap
    }
  } catch {
    // A corrupt or unavailable cache is not worth surfacing; start empty.
    store = {}
  }
  return store
}

let flushTimer: ReturnType<typeof setTimeout> | null = null

/**
 * Writing on every resolve would hit disk once per row during a fast scroll,
 * so batch into one write per idle moment.
 */
function scheduleFlush(): void {
  if (flushTimer) return
  flushTimer = setTimeout(() => {
    flushTimer = null
    const map = load()
    try {
      const keys = Object.keys(map)
      if (keys.length > MAX_ENTRIES) {
        // Drop the oldest resolutions; they are the cheapest to rebuild.
        const sorted = keys.sort((a, b) => map[b].at - map[a].at).slice(0, MAX_ENTRIES)
        const trimmed: CacheMap = {}
        for (const k of sorted) trimmed[k] = map[k]
        store = trimmed
      }
      localStorage.setItem(STORAGE_KEY, JSON.stringify(store))
    } catch {
      // Quota or private-mode failures leave the in-memory cache intact.
    }
  }, 1000)
}

function remember(key: string, url: string | null): string | null {
  load()[key] = { url, at: Date.now() }
  scheduleFlush()
  return url
}

/** The account id or login inside a GitHub no-reply address, if present. */
function githubNoReply(email: string): { id?: string; login?: string } | null {
  const match = email.match(/^(?:(\d+)\+)?([^@]+)@users\.noreply\.github\.com$/)
  if (!match) return null
  return match[1] ? { id: match[1] } : { login: match[2] }
}

async function gravatarHash(email: string): Promise<string> {
  const bytes = new TextEncoder().encode(email)
  const digest = await crypto.subtle.digest('SHA-256', bytes)
  return [...new Uint8Array(digest)].map((b) => b.toString(16).padStart(2, '0')).join('')
}

/** Resolves once an image either decodes or fails, without touching the DOM. */
function imageLoads(url: string): Promise<boolean> {
  return new Promise((resolve) => {
    const img = new Image()
    img.onload = () => resolve(true)
    img.onerror = () => resolve(false)
    img.src = url
  })
}

async function resolve(email: string): Promise<string | null> {
  const github = githubNoReply(email)
  if (github) {
    // Both forms redirect to the same avatars.githubusercontent.com image, and
    // the address itself proves the account exists, so no probe is needed.
    return github.id
      ? `https://avatars.githubusercontent.com/u/${github.id}`
      : `https://github.com/${encodeURIComponent(github.login!)}.png`
  }

  // d=404 turns "no Gravatar for this address" into a load error we can cache
  // as a negative result, instead of a default image we would rather not show.
  const base = `https://gravatar.com/avatar/${await gravatarHash(email)}`
  return (await imageLoads(`${base}?s=64&d=404`)) ? base : null
}

/** Append the pixel size each host expects, so one cached entry serves all sizes. */
function sized(url: string, px: number): string {
  if (url.startsWith('https://avatars.githubusercontent.com/')) return `${url}?s=${px}`
  if (url.endsWith('.png')) return `${url}?size=${px}`
  return `${url}?s=${px}&d=404`
}

/**
 * The avatar URL for a commit author at the requested pixel size, or null when
 * they have no picture anywhere. Cached across sessions by email.
 */
export async function avatarUrl(email: string, px: number): Promise<string | null> {
  const key = email.trim().toLowerCase()
  if (!key) return null

  const hit = load()[key]
  if (hit && Date.now() - hit.at < TTL_MS) return hit.url ? sized(hit.url, px) : null

  let pending = inflight.get(key)
  if (!pending) {
    pending = resolve(key)
      .then((url) => remember(key, url))
      .finally(() => inflight.delete(key))
    inflight.set(key, pending)
  }
  const url = await pending
  return url ? sized(url, px) : null
}
