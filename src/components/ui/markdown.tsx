import ReactMarkdown from 'react-markdown'
import remarkGfm from 'remark-gfm'
import { REHYPE_PLUGINS } from '@/lib/markdownPipeline'
import { cn } from '@/lib/utils'

/**
 * GitHub-flavored markdown, styled to match the app.
 *
 * Raw HTML is parsed and filtered against an allowlist; see
 * `@/lib/markdownPipeline` for why, and for the order the plugins must run in.
 */
export function Markdown({
  text,
  empty = 'Nothing written yet.',
  className,
}: {
  text: string
  /** Shown when `text` is blank -- an empty area reads as broken. */
  empty?: string
  className?: string
}) {
  if (!text.trim()) {
    return <p className="text-xs italic text-muted-foreground">{empty}</p>
  }
  return (
    <div
      className={cn(
        'select-text text-[0.78125rem] leading-relaxed text-foreground',
        '[&_p]:mb-3 [&_p:last-child]:mb-0',
        '[&_h1]:mb-2 [&_h1]:mt-4 [&_h1]:text-sm [&_h1]:font-bold',
        '[&_h2]:mb-2 [&_h2]:mt-4 [&_h2]:text-[0.8125rem] [&_h2]:font-bold',
        '[&_h3]:mb-1.5 [&_h3]:mt-3 [&_h3]:text-xs [&_h3]:font-bold',
        '[&_ul]:mb-3 [&_ul]:list-disc [&_ul]:pl-5 [&_ol]:mb-3 [&_ol]:list-decimal [&_ol]:pl-5',
        '[&_li]:mb-1',
        '[&_code]:rounded [&_code]:bg-panel3 [&_code]:px-1 [&_code]:py-px [&_code]:font-mono [&_code]:text-2xs',
        '[&_pre]:mb-3 [&_pre]:overflow-x-auto [&_pre]:rounded-md [&_pre]:border [&_pre]:border-border [&_pre]:bg-background [&_pre]:p-2.5',
        '[&_pre_code]:bg-transparent [&_pre_code]:p-0',
        '[&_a]:text-accent-text [&_a]:underline-offset-2 hover:[&_a]:underline',
        '[&_blockquote]:mb-3 [&_blockquote]:border-l-2 [&_blockquote]:border-border [&_blockquote]:pl-3 [&_blockquote]:text-sub',
        '[&_img]:max-w-full [&_hr]:my-3 [&_hr]:border-border',
        // Collapsibles: bots put release notes and changelogs in these, so they
        // have to read as something you can click rather than as a bare line.
        '[&_details]:mb-3 [&_details]:rounded-md [&_details]:border [&_details]:border-border [&_details]:bg-panel2/60 [&_details]:px-3 [&_details]:py-2',
        '[&_summary]:cursor-pointer [&_summary]:select-none [&_summary]:font-semibold [&_summary]:text-foreground hover:[&_summary]:text-accent-text',
        '[&_details[open]_summary]:mb-2 [&_details[open]_summary]:border-b [&_details[open]_summary]:border-border [&_details[open]_summary]:pb-2',
        '[&_table]:mb-3 [&_table]:border-collapse [&_th]:border [&_th]:border-border [&_th]:px-2 [&_th]:py-1 [&_td]:border [&_td]:border-border [&_td]:px-2 [&_td]:py-1',
        className
      )}
    >
      <ReactMarkdown remarkPlugins={[remarkGfm]} rehypePlugins={REHYPE_PLUGINS}>
        {text}
      </ReactMarkdown>
    </div>
  )
}
