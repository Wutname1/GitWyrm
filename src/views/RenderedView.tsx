import { useState } from 'react'
import { Check, Copy } from 'lucide-react'
import { shortSha } from '@/lib/gitDisplay'
import { useFileContent } from '@/hooks/useGitQueries'
import { useUiStore } from '@/stores/uiStore'
import { useActiveRepo } from '@/stores/workspaceStore'
import { TooltipButton } from '@/components/ui/tooltip'
import { FileViewHeader } from '@/components/domain/file/FileViewHeader'
import { MarkdownPreview } from '@/components/domain/file/MarkdownPreview'

/**
 * A Markdown file laid out as a page, from the same version Raw shows: the
 * commit you came from, or the working copy when there is no commit in context.
 */
export function RenderedView() {
  const repo = useActiveRepo()
  const target = useUiStore((s) => s.fileTarget)
  const [copied, setCopied] = useState(false)
  const file = useFileContent(repo?.id ?? null, target?.path ?? null, target?.sha ?? null)

  if (!target) return null

  const data = file.data
  const showPage = data != null && !data.binary && !data.too_large

  const copy = async () => {
    if (!data?.text) return
    await navigator.clipboard.writeText(data.text)
    setCopied(true)
    setTimeout(() => setCopied(false), 1600)
  }

  return (
    <>
      <FileViewHeader
        path={target.path}
        mode="rendered"
        pinnedLabel={target.sha ? `at ${shortSha(target.sha)}` : 'working copy'}
      >
        {showPage && (
          <TooltipButton
            onClick={copy}
            tooltip={copied ? 'Copied' : 'Copy the Markdown source'}
            className="flex size-6 flex-none items-center justify-center rounded-[5px] border border-border bg-panel2 text-xs text-sub hover:border-muted-foreground hover:bg-panel3"
          >
            {copied ? <Check size={12} className="text-accent-text" /> : <Copy size={12} />}
          </TooltipButton>
        )}
      </FileViewHeader>

      <div className="min-h-0 flex-1 overflow-hidden">
        {file.isLoading && (
          <div className="p-4 text-center text-xs text-muted-foreground">Reading file…</div>
        )}
        {file.isError && (
          <div className="p-4 text-center text-xs text-removed">
            {(file.error as Error).message}
          </div>
        )}
        {data?.binary && (
          <div className="p-4 text-center text-xs text-muted-foreground">
            This is a binary file, so there is no text to show.
          </div>
        )}
        {data?.too_large && (
          <div className="p-4 text-center text-xs text-muted-foreground">
            This file is too big to open here. Open it in your editor instead.
          </div>
        )}
        {showPage && data.text.trim().length === 0 && (
          <div className="p-4 text-center text-xs text-muted-foreground">This file is empty.</div>
        )}
        {showPage && data.text.trim().length > 0 && (
          <MarkdownPreview key={`${target.path}:${target.sha ?? 'work'}`} text={data.text} />
        )}
      </div>
    </>
  )
}
