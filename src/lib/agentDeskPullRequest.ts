/**
 * Put a drafted title and body into a host's new-pull-request URL.
 *
 * Every host GitWyrm recognises reads these from the query string on its own
 * compare page: GitHub, GitLab (as `merge_request[...]`), Bitbucket and Azure
 * DevOps all prefill their form from a link. That is the whole mechanism.
 * GitWyrm opens a page; the person presses the host's own button. Nothing
 * here pushes a branch or calls a host write API.
 *
 * An unrecognised host keeps its plain compare URL rather than gaining
 * parameters it would ignore or choke on.
 */
/**
 * The longest URL worth handing to the operating system.
 *
 * Windows opens the page through `ShellExecute`, which stops accepting a URL
 * somewhere around 2048 characters -- and Windows is the platform GitWyrm
 * ships an installer for, so it is the ceiling that binds. Past it the browser
 * simply does not open, or opens something truncated; either way the person
 * pressed a button and got nothing.
 *
 * 2000 rather than 2048, to leave room for any escaping the opener adds on the
 * way out.
 */
const MAX_URL_LENGTH = 2000

export interface PullRequestLink {
  /** The URL to open. */
  url: string
  /**
   * Whether the description travelled in it.
   *
   * `false` means the link would have been too long for the system to open,
   * so it carries the title only and the description has to reach the host
   * another way. The caller is expected to put it on the clipboard and say so
   * -- the same answer this already gives when updating an existing pull
   * request, whose page ignores these parameters entirely.
   *
   * The description is never shortened to make it fit. A half-written
   * description that looks complete on the host's page is worse than an
   * honest one that arrives by clipboard.
   */
  bodyFitsInLink: boolean
}

export function pullRequestUrlWithDraft(
  compareUrl: string,
  title: string,
  body: string
): PullRequestLink {
  const withBody = buildUrl(compareUrl, title, body)

  // An unrecognised host (or a compare address that is not a URL at all) gets
  // its plain link back, with no parameters on it -- so the description did
  // not travel, whatever its length. Reporting that it did would leave the
  // caller believing the host has text it never received.
  const trimmedBody = body.trim()
  if (trimmedBody && withBody === compareUrl) {
    return { url: compareUrl, bodyFitsInLink: false }
  }

  if (withBody.length <= MAX_URL_LENGTH) {
    return { url: withBody, bodyFitsInLink: true }
  }
  return {
    url: buildUrl(compareUrl, title, ''),
    bodyFitsInLink: false,
  }
}

function buildUrl(compareUrl: string, title: string, body: string): string {
  let url: URL
  try {
    url = new URL(compareUrl)
  } catch {
    // Not a URL this can safely edit; hand back exactly what was given.
    return compareUrl
  }

  const host = url.hostname.toLowerCase()
  const trimmedTitle = title.trim()
  const trimmedBody = body.trim()

  if (host === 'github.com' || host.endsWith('.github.com')) {
    // GitHub reads `title` and `body`, and needs `expand=1` to open the form
    // rather than the comparison view.
    url.searchParams.set('expand', '1')
    if (trimmedTitle) url.searchParams.set('title', trimmedTitle)
    if (trimmedBody) url.searchParams.set('body', trimmedBody)
    return url.toString()
  }

  if (host === 'gitlab.com' || host.includes('gitlab')) {
    if (trimmedTitle) url.searchParams.set('merge_request[title]', trimmedTitle)
    if (trimmedBody) url.searchParams.set('merge_request[description]', trimmedBody)
    return url.toString()
  }

  if (host === 'bitbucket.org' || host.endsWith('.bitbucket.org')) {
    if (trimmedTitle) url.searchParams.set('title', trimmedTitle)
    if (trimmedBody) url.searchParams.set('description', trimmedBody)
    return url.toString()
  }

  if (host.includes('dev.azure.com') || host.includes('visualstudio.com')) {
    if (trimmedTitle) url.searchParams.set('title', trimmedTitle)
    if (trimmedBody) url.searchParams.set('description', trimmedBody)
    return url.toString()
  }

  return compareUrl
}
