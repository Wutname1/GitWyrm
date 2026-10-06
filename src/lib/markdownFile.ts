/**
 * Rules for showing a Markdown file from the repository as a rendered page.
 *
 * This is a different job from `markdownPipeline`, which renders text written
 * on a hosting site (pull request bodies, comments). A repo file is shown with
 * no raw HTML at all, its relative links point at other files rather than web
 * pages, and its relative images point at files the webview must not request.
 */
import type { Element, ElementContent, Root } from 'hast'

const MARKDOWN_EXTENSIONS = ['.md', '.markdown', '.mdx']

/** True for files the Rendered view is offered for. */
export function isMarkdownPath(path: string): boolean {
  const lower = path.toLowerCase()
  return MARKDOWN_EXTENSIONS.some((ext) => lower.endsWith(ext))
}

/**
 * How an image in a repo Markdown file is shown.
 *
 * - `remote` -- an http(s) URL, loaded like any web image.
 * - `placeholder` -- anything else: a path inside the repo, or a source the
 *   URL sanitizer already emptied (`data:`, `javascript:`). Resolving a repo
 *   path would mean the webview requesting local files, so the alt text stands
 *   in for it.
 */
export type MarkdownImageAction = { kind: 'remote'; url: string } | { kind: 'placeholder' }

export function markdownImageAction(src?: string): MarkdownImageAction {
  const target = src?.trim()
  if (!target) return { kind: 'placeholder' }
  let parsed: URL
  try {
    parsed = new URL(target)
  } catch {
    return { kind: 'placeholder' }
  }
  if (parsed.protocol !== 'http:' && parsed.protocol !== 'https:') return { kind: 'placeholder' }
  return { kind: 'remote', url: parsed.toString() }
}

/**
 * Turn heading text into the anchor hosting sites give it, so the `#install`
 * links a README writes for itself land on the right heading.
 */
export function slugifyHeading(text: string): string {
  return text
    .trim()
    .toLowerCase()
    .replace(/[^\p{L}\p{M}\p{N}\p{Pc} -]/gu, '')
    .replace(/ /g, '-')
}

/** Data attribute the anchors live on, rather than `id`, so they can never clash with the app's own ids. */
export const HEADING_ANCHOR_ATTRIBUTE = 'data-md-anchor'

function textOf(nodes: ElementContent[]): string {
  return nodes
    .map((node) => {
      if (node.type === 'text') return node.value
      if (node.type === 'element') return textOf(node.children)
      return ''
    })
    .join('')
}

const HEADING_TAGS = new Set(['h1', 'h2', 'h3', 'h4', 'h5', 'h6'])

/**
 * Rehype plugin that tags every heading with its anchor. Repeated headings get
 * `-1`, `-2` and so on, the way hosting sites number them.
 */
export function rehypeHeadingAnchors() {
  return (tree: Root) => {
    const seen = new Map<string, number>()
    const visit = (node: Root | Element) => {
      for (const child of node.children) {
        if (child.type !== 'element') continue
        if (HEADING_TAGS.has(child.tagName)) {
          const base = slugifyHeading(textOf(child.children))
          const count = seen.get(base) ?? 0
          seen.set(base, count + 1)
          child.properties.dataMdAnchor = count === 0 ? base : `${base}-${count}`
        } else {
          visit(child)
        }
      }
    }
    visit(tree)
  }
}
