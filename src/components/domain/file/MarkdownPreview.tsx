import { useCallback, useMemo, useRef, useState, type MouseEvent, type ReactNode } from 'react'
import ReactMarkdown, { type Components } from 'react-markdown'
import remarkGfm from 'remark-gfm'
import { ImageOff } from 'lucide-react'
import { cn } from '@/lib/utils'
import { markdownLinkAction } from '@/lib/markdownPipeline'
import {
  HEADING_ANCHOR_ATTRIBUTE,
  markdownImageAction,
  rehypeHeadingAnchors,
} from '@/lib/markdownFile'
import { openWebUrl } from '@/lib/remoteWeb'

const REHYPE_PLUGINS = [rehypeHeadingAnchors]

function MarkdownImage({ src, alt, title }: { src?: string; alt?: string; title?: string }) {
  const [failed, setFailed] = useState(false)
  const action = markdownImageAction(src)
  if (action.kind === 'remote' && !failed) {
    return (
      <img
        src={action.url}
        alt={alt ?? ''}
        title={title}
        loading="lazy"
        referrerPolicy="no-referrer"
        onError={() => setFailed(true)}
      />
    )
  }
  return (
    <span
      title={src ? `Image not shown: ${src}` : 'Image not shown'}
      className="inline-flex max-w-full items-center gap-1 rounded border border-dashed border-border bg-panel2 px-1.5 py-px align-middle text-2xs text-muted-foreground"
    >
      <ImageOff size={11} className="flex-none" />
      <span className="truncate">{alt || 'Image'}</span>
    </span>
  )
}

/**
 * A Markdown file from the repository, laid out as a page.
 *
 * Raw HTML in the file is dropped rather than rendered: the file can come from
 * any repository, and the Raw tab is one click away for the exact text. Links
 * never navigate the app window -- web links open in the browser, `#anchors`
 * scroll within the page, and links to other repo files do nothing.
 */
export function MarkdownPreview({ text }: { text: string }) {
  const scrollerRef = useRef<HTMLDivElement>(null)
  const rootRef = useRef<HTMLDivElement>(null)

  const scrollToAnchor = useCallback((fragment: string) => {
    const scroller = scrollerRef.current
    const root = rootRef.current
    if (!scroller || !root) return
    let name = fragment
    try {
      name = decodeURIComponent(fragment)
    } catch {
      // Malformed escapes: match the fragment as written.
    }
    const candidates = [name, name.toLowerCase()].map(CSS.escape)
    const target = candidates
      .flatMap((value) => [`[${HEADING_ANCHOR_ATTRIBUTE}="${value}"]`, `[id="${value}"]`])
      .map((selector) => root.querySelector(selector))
      .find((el) => el != null)
    if (!target) return
    // Scroll only this pane; scrollIntoView would also nudge the app's own
    // clipped containers.
    const offset = target.getBoundingClientRect().top - scroller.getBoundingClientRect().top
    scroller.scrollTo({ top: scroller.scrollTop + offset - 12 })
  }, [])

  const components = useMemo<Components>(
    () => ({
      // `rest` carries the ids and aria links GFM footnotes rely on.
      a: ({ node: _node, href, children, onClick: _onClick, ...rest }) => {
        const action = markdownLinkAction(href)
        const follow = (event: MouseEvent<HTMLAnchorElement>) => {
          event.preventDefault()
          if (event.type !== 'click') return
          if (action.kind === 'inPage' && href) scrollToAnchor(href.trim().slice(1))
          if (action.kind === 'external') openWebUrl(action.url, 'that link')
        }
        return (
          <a
            {...rest}
            href={href}
            onClick={follow}
            onAuxClick={follow}
            title={action.kind === 'ignore' && href ? `${href} (links to files are not followed here)` : rest.title}
            className={cn(rest.className, action.kind === 'ignore' && 'cursor-default')}
          >
            {children}
          </a>
        )
      },
      img: ({ src, alt, title }) => (
        <MarkdownImage src={typeof src === 'string' ? src : undefined} alt={alt} title={title} />
      ),
      table: ({ children }: { children?: ReactNode }) => (
        <div className="mb-4 overflow-x-auto">
          <table>{children}</table>
        </div>
      ),
    }),
    [scrollToAnchor]
  )

  return (
    <div ref={scrollerRef} className="h-full overflow-auto bg-background">
      <div
        ref={rootRef}
        className={cn(
          'mx-auto max-w-[52rem] select-text px-8 py-6 text-sm leading-relaxed text-foreground',
          '[&>*:first-child]:mt-0',
          '[&_p]:mb-4',
          '[&_h1]:mb-4 [&_h1]:mt-6 [&_h1]:border-b [&_h1]:border-border [&_h1]:pb-2 [&_h1]:text-2xl [&_h1]:font-semibold [&_h1]:leading-tight',
          '[&_h2]:mb-3 [&_h2]:mt-6 [&_h2]:border-b [&_h2]:border-border [&_h2]:pb-1.5 [&_h2]:text-xl [&_h2]:font-semibold [&_h2]:leading-tight',
          '[&_h3]:mb-2 [&_h3]:mt-5 [&_h3]:text-base [&_h3]:font-semibold',
          '[&_h4]:mb-2 [&_h4]:mt-4 [&_h4]:text-sm [&_h4]:font-semibold',
          '[&_h5]:mb-2 [&_h5]:mt-4 [&_h5]:text-xs [&_h5]:font-semibold',
          '[&_h6]:mb-2 [&_h6]:mt-4 [&_h6]:text-xs [&_h6]:font-semibold [&_h6]:text-sub',
          '[&_ul]:mb-4 [&_ul]:list-disc [&_ul]:pl-6 [&_ol]:mb-4 [&_ol]:list-decimal [&_ol]:pl-6',
          '[&_li]:mb-1 [&_li>ul]:mb-0 [&_li>ol]:mb-0 [&_li>ul]:mt-1 [&_li>ol]:mt-1',
          '[&_.contains-task-list]:list-none [&_.contains-task-list]:pl-1',
          '[&_.task-list-item_input]:mr-2 [&_.task-list-item_input]:align-middle [&_.task-list-item_input]:accent-primary',
          '[&_code]:rounded [&_code]:bg-panel3 [&_code]:px-1 [&_code]:py-px [&_code]:font-mono [&_code]:text-[0.8125rem]',
          '[&_pre]:mb-4 [&_pre]:overflow-x-auto [&_pre]:rounded-md [&_pre]:border [&_pre]:border-border [&_pre]:bg-panel2 [&_pre]:p-3 [&_pre]:leading-normal',
          '[&_pre_code]:bg-transparent [&_pre_code]:p-0',
          '[&_a]:text-accent-text [&_a]:underline-offset-2 hover:[&_a]:underline',
          '[&_blockquote]:mb-4 [&_blockquote]:border-l-2 [&_blockquote]:border-border [&_blockquote]:pl-4 [&_blockquote]:text-sub',
          '[&_img]:inline [&_img]:max-w-full',
          '[&_hr]:my-6 [&_hr]:border-border',
          '[&_table]:border-collapse [&_th]:border [&_th]:border-border [&_th]:bg-panel2 [&_th]:px-3 [&_th]:py-1.5 [&_th]:text-left [&_th]:font-semibold [&_td]:border [&_td]:border-border [&_td]:px-3 [&_td]:py-1.5',
          '[&_del]:text-sub',
          '[&_.footnotes]:mt-8 [&_.footnotes]:border-t [&_.footnotes]:border-border [&_.footnotes]:pt-4 [&_.footnotes]:text-xs [&_.footnotes]:text-sub'
        )}
      >
        <ReactMarkdown
          remarkPlugins={[remarkGfm]}
          rehypePlugins={REHYPE_PLUGINS}
          skipHtml
          components={components}
        >
          {text}
        </ReactMarkdown>
      </div>
    </div>
  )
}
