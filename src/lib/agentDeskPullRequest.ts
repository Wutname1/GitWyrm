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
export function pullRequestUrlWithDraft(compareUrl: string, title: string, body: string): string {
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
