/**
 * The markdown renderer is a security boundary: it shows text other people and
 * tools wrote, inside an app that can read the filesystem. These run the real
 * pipeline -- same plugins, same order as the component -- and assert that
 * useful formatting survives and dangerous markup does not.
 *
 * They are deliberately blunt about what "does not survive" means: the assertion
 * is on the rendered HTML string, so a plugin reorder that reopens the hole
 * fails here rather than shipping.
 */
import { describe, expect, it } from 'vitest'
import { unified } from 'unified'
import remarkParse from 'remark-parse'
import remarkGfm from 'remark-gfm'
import remarkRehype from 'remark-rehype'
import rehypeStringify from 'rehype-stringify'
import { markdownLinkAction, REHYPE_PLUGINS } from './markdownPipeline'

/** Render markdown the way the Markdown component does. */
function render(markdown: string): string {
  return unified()
    .use(remarkParse)
    .use(remarkGfm)
    // `allowDangerousHtml` only means "pass the raw HTML along to rehype"; it is
    // what makes rehypeRaw able to see it at all. The sanitizer is what makes
    // that safe, which is the thing these tests pin down.
    .use(remarkRehype, { allowDangerousHtml: true })
    .use(REHYPE_PLUGINS)
    .use(rehypeStringify)
    .processSync(markdown)
    .toString()
}

describe('formatting that must survive', () => {
  it('renders collapsibles as real elements', () => {
    // The exact shape dependency bots use for release notes.
    const html = render('<details><summary>Release notes</summary><p>Fixed a bug.</p></details>')
    expect(html).toContain('<details>')
    expect(html).toContain('<summary>Release notes</summary>')
    expect(html).toContain('Fixed a bug.')
  })

  it('keeps inline formatting and links inside HTML blocks', () => {
    const html = render('<blockquote><code>vite dev</code> <a href="https://x.test">docs</a></blockquote>')
    expect(html).toContain('<blockquote>')
    expect(html).toContain('<code>vite dev</code>')
    expect(html).toContain('href="https://x.test"')
  })

  it('still renders ordinary markdown and GFM tables', () => {
    const html = render('# Title\n\n| a | b |\n| - | - |\n| 1 | 2 |')
    expect(html).toContain('<h1>Title</h1>')
    expect(html).toContain('<table>')
  })
})

describe('markup that must not survive', () => {
  it('drops script tags and their contents', () => {
    const html = render('before <script>globalThis.pwned = 1</script> after')
    expect(html).not.toContain('<script')
    expect(html).not.toContain('pwned')
    expect(html).toContain('before')
  })

  it('drops inline event handlers', () => {
    const html = render('<img src="https://x.test/a.png" onerror="globalThis.pwned = 1">')
    expect(html).not.toContain('onerror')
    expect(html).not.toContain('pwned')
  })

  it('drops javascript: urls while keeping the link text', () => {
    const html = render('<a href="javascript:globalThis.pwned = 1">click me</a>')
    expect(html).not.toContain('javascript:')
    expect(html).toContain('click me')
  })

  it('drops iframes', () => {
    const html = render('<iframe src="https://evil.test"></iframe>')
    expect(html).not.toContain('<iframe')
    expect(html).not.toContain('evil.test')
  })

  it('drops javascript: urls written as markdown links too', () => {
    // The markdown path is separate from the raw-HTML path; both are filtered.
    // No spaces in the URL, or markdown declines to parse it as a link and the
    // test passes on inert text instead of on the sanitizer.
    const html = render('[click me](javascript:alert(1))')
    expect(html).toContain('<a')
    expect(html).not.toContain('javascript:')
    expect(html).toContain('click me')
  })

  it('drops style attributes, so content cannot repaint the app', () => {
    const html = render('<p style="position:fixed;inset:0;background:red">hi</p>')
    expect(html).not.toContain('style=')
    expect(html).toContain('hi')
  })
})

describe('markdownLinkAction', () => {
  const PR = 'https://github.com/owner/repo/pull/1'

  it('sends an absolute link to the browser', () => {
    expect(markdownLinkAction('https://example.com/docs', PR)).toEqual({
      kind: 'external',
      url: 'https://example.com/docs',
    })
  })

  it('keeps an in-page fragment in the document', () => {
    // These scroll within the body already on screen; handing them to the
    // browser would open a second copy of the page for no reason.
    expect(markdownLinkAction('#issuecomment-123', PR)).toEqual({ kind: 'inPage' })
  })

  it('resolves a host-relative link against the page it was written on', () => {
    // GitHub writes these constantly. Dropping them would make cross-links
    // between pull requests dead text.
    expect(markdownLinkAction('/owner/repo/issues/7', PR)).toEqual({
      kind: 'external',
      url: 'https://github.com/owner/repo/issues/7',
    })
  })

  it('ignores a relative link when there is no page to resolve it against', () => {
    // Guessing a host is worse than doing nothing: it would send the user
    // somewhere the text never pointed.
    expect(markdownLinkAction('/owner/repo/issues/7')).toEqual({ kind: 'ignore' })
  })

  it('ignores an empty or whitespace href', () => {
    expect(markdownLinkAction(undefined)).toEqual({ kind: 'ignore' })
    expect(markdownLinkAction('   ', PR)).toEqual({ kind: 'ignore' })
  })

  it.each(['javascript:alert(1)', 'file:///C:/Windows/System32', 'data:text/html,<script>'])(
    'refuses to open %s',
    (href) => {
      // The sanitizer strips these upstream, but this helper is the step that
      // hands a string to the OS, so it carries its own lock. `new URL` parses
      // all three without complaint, so nothing here is hypothetical.
      expect(markdownLinkAction(href, PR)).toEqual({ kind: 'ignore' })
    }
  )

  it('allows mailto, which PR bodies use for contact links', () => {
    expect(markdownLinkAction('mailto:someone@example.com', PR)).toEqual({
      kind: 'external',
      url: 'mailto:someone@example.com',
    })
  })
})
