import type { RemoteProvider } from '@/lib/remoteProvider'

export interface AuthorProfileLink {
  url: string
  label: string
}

/**
 * A no-reply address encodes the account name, so it is the only email we can
 * turn into a profile with certainty: "1234+octocat@users.noreply.github.com".
 */
function githubHandleFromEmail(email: string): string | null {
  const match = email.trim().toLowerCase().match(/^(?:\d+\+)?([^@]+)@users\.noreply\.github\.com$/)
  return match ? match[1] : null
}

/**
 * Where to send someone who wants to know more about this author.
 *
 * When the handle is knowable we link the profile page directly; otherwise we
 * fall back to the host's commit search for the address, which is the closest
 * thing to "who is this" that works without an API call. Hosts without a
 * usable route return null and the card just omits the link.
 */
export function authorProfileLink(
  provider: RemoteProvider,
  email: string,
  repositoryUrl: string | null
): AuthorProfileLink | null {
  const address = email.trim()
  if (!address) return null

  switch (provider) {
    case 'github': {
      const handle = githubHandleFromEmail(address)
      if (handle) return { url: `https://github.com/${encodeURIComponent(handle)}`, label: 'GitHub profile' }
      return {
        url: `https://github.com/search?q=${encodeURIComponent(`author-email:${address}`)}&type=commits`,
        label: 'Find on GitHub',
      }
    }
    case 'gitlab':
      return {
        url: `https://gitlab.com/search?scope=users&search=${encodeURIComponent(address)}`,
        label: 'Find on GitLab',
      }
    case 'bitbucket':
      if (!repositoryUrl) return null
      return { url: `${repositoryUrl}/commits`, label: 'Commits on Bitbucket' }
    default:
      return null
  }
}
