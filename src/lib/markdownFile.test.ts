import { describe, expect, it } from 'vitest'
import { unified } from 'unified'
import remarkParse from 'remark-parse'
import remarkGfm from 'remark-gfm'
import remarkRehype from 'remark-rehype'
import rehypeStringify from 'rehype-stringify'
import {
  isMarkdownPath,
  markdownImageAction,
  rehypeHeadingAnchors,
  slugifyHeading,
} from './markdownFile'

describe('isMarkdownPath', () => {
  it('accepts Markdown extensions in any case', () => {
    expect(isMarkdownPath('README.md')).toBe(true)
    expect(isMarkdownPath('docs/Guide.MD')).toBe(true)
    expect(isMarkdownPath('notes.markdown')).toBe(true)
    expect(isMarkdownPath('site/page.mdx')).toBe(true)
  })

  it('rejects other files', () => {
    expect(isMarkdownPath('src/md.ts')).toBe(false)
    expect(isMarkdownPath('README')).toBe(false)
    expect(isMarkdownPath('archive.md.zip')).toBe(false)
  })
})

describe('markdownImageAction', () => {
  it('loads web images', () => {
    expect(markdownImageAction('https://img.test/badge.svg')).toEqual({
      kind: 'remote',
      url: 'https://img.test/badge.svg',
    })
  })

  it('never resolves repo paths or other schemes', () => {
    expect(markdownImageAction('docs/screenshot.png')).toEqual({ kind: 'placeholder' })
    expect(markdownImageAction('/assets/logo.png')).toEqual({ kind: 'placeholder' })
    expect(markdownImageAction('file:///C:/secret.png')).toEqual({ kind: 'placeholder' })
    expect(markdownImageAction('')).toEqual({ kind: 'placeholder' })
    expect(markdownImageAction(undefined)).toEqual({ kind: 'placeholder' })
  })
})

describe('heading anchors', () => {
  it('slugs heading text the way hosting sites do', () => {
    expect(slugifyHeading('Getting Started')).toBe('getting-started')
    expect(slugifyHeading('What is `git rebase`?')).toBe('what-is-git-rebase')
    expect(slugifyHeading('  C++ & Rust  ')).toBe('c--rust')
  })

  it('numbers repeated headings', () => {
    const html = unified()
      .use(remarkParse)
      .use(remarkGfm)
      .use(remarkRehype)
      .use(rehypeHeadingAnchors)
      .use(rehypeStringify)
      .processSync('# Setup\n\n## Setup\n\n### Usage *notes*')
      .toString()
    expect(html).toContain('<h1 data-md-anchor="setup">')
    expect(html).toContain('<h2 data-md-anchor="setup-1">')
    expect(html).toContain('<h3 data-md-anchor="usage-notes">')
  })
})
