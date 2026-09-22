/**
 * The rehype half of the markdown pipeline, kept here so it can be tested.
 *
 * This is a security boundary. The markdown GitWyrm renders is written by other
 * people and tools -- pull request bodies, issue comments, bot release notes,
 * openspec proposals -- and the app has filesystem access, so HTML that reaches
 * the DOM unfiltered is a real hole rather than a theoretical one.
 *
 * Raw HTML used to be escaped to visible text instead, which was safe but made
 * ordinary pull requests unreadable: dependency bots wrap release notes in
 * `<details>`, and those bodies rendered as a wall of tags.
 */
import rehypeRaw from 'rehype-raw'
import rehypeSanitize, { defaultSchema } from 'rehype-sanitize'
import type { PluggableList } from 'unified'

/**
 * Parse raw HTML, then filter it against an allowlist.
 *
 * ORDER MATTERS. `rehypeRaw` turns the HTML string into real nodes and
 * `rehypeSanitize` filters those nodes, so raw must run first. Reversed, the
 * sanitizer inspects a tree where the HTML is still one opaque text node, passes
 * it through untouched, and the unfiltered markup is then mounted -- which looks
 * correct for benign input and is completely open to hostile input.
 *
 * `defaultSchema` is the allowlist GitHub sanitizes with: it keeps the
 * formatting tags that appear in practice (`details`, `summary`, `blockquote`,
 * `code`, tables, images) and drops `script`, `iframe`, every `on*` handler, and
 * any href whose protocol is not http/https/mailto.
 */
export const REHYPE_PLUGINS: PluggableList = [rehypeRaw, [rehypeSanitize, defaultSchema]]

/**
 * How a link inside rendered markdown should be followed.
 *
 * A bare `<a href>` in a Tauri window navigates the window itself, so following
 * a link in place replaced the whole app with that website and left no way
 * back. Every link here comes from someone else's text, so the decision of
 * where it may go belongs in one tested place rather than in a click handler.
 *
 * - `inPage` -- a `#fragment`, which moves within the text already on screen.
 * - `external` -- open `url` in the browser.
 * - `ignore` -- nothing safe to do: an empty href, or one that is not a URL
 *   even after resolving against the page it was written on.
 */
export type MarkdownLinkAction =
  | { kind: 'inPage' }
  | { kind: 'external'; url: string }
  | { kind: 'ignore' }

/**
 * `baseUrl` is the page the markdown was written on, which is what makes the
 * relative links hosts write (`/owner/repo/pull/1`) resolvable. Without one
 * those are ignored rather than guessed at, since the wrong host is worse than
 * a link that does nothing.
 */
export function markdownLinkAction(href?: string, baseUrl?: string): MarkdownLinkAction {
  const target = href?.trim()
  if (!target) return { kind: 'ignore' }
  if (target.startsWith('#')) return { kind: 'inPage' }
  let parsed: URL
  try {
    parsed = new URL(target, baseUrl)
  } catch {
    return { kind: 'ignore' }
  }
  // `new URL` parses `javascript:` and `file:` perfectly happily, and this is
  // the step that hands a string to the operating system to open. The
  // sanitizer already drops those hrefs upstream, but a protocol allowlist
  // belongs on the side that does the opening rather than being borrowed from
  // a plugin two layers away.
  if (!SAFE_LINK_PROTOCOLS.has(parsed.protocol)) return { kind: 'ignore' }
  return { kind: 'external', url: parsed.toString() }
}

const SAFE_LINK_PROTOCOLS = new Set(['http:', 'https:', 'mailto:'])
