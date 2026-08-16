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
