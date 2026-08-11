import { lazy, Suspense, useState } from 'react'
import { Check, Copy, WrapText } from 'lucide-react'
import { cn } from '@/lib/utils'
import { shortSha } from '@/lib/gitDisplay'
import { languageLabel } from '@/lib/codeLanguage'
import { useFileContent } from '@/hooks/useGitQueries'
import { useUiStore } from '@/stores/uiStore'
import { useActiveRepo } from '@/stores/workspaceStore'
import { TooltipButton } from '@/components/ui/tooltip'
import { FileViewHeader } from '@/components/domain/file/FileViewHeader'

/**
 * CodeMirror and its grammars are only paid for when someone actually opens
 * Raw. Most sessions read diffs and never come here.
 */
const CodeViewer = lazy(async () => ({
  default: (await import('@/components/domain/file/CodeViewer')).CodeViewer,
}))

/** Bytes as something a person reads, for the "too large" message. */
function formatSize(bytes: number): string {
  if (bytes >= 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`
  if (bytes >= 1024) return `${Math.round(bytes / 1024)} KB`
  return `${bytes} bytes`
}

/**
 * The whole file, as it stood at one point in time -- the version in the commit
 * you came from, or the working copy when there is no commit in context.
 *
 * Diff answers "what changed here", and that is the right question most of the
 * time. It is the wrong one when you need the surrounding code to understand
 * the change, or simply want to read a file the way it was six months ago.
 */
export function RawView() {
  const repo = useActiveRepo()
  const target = useUiStore((s) => s.fileTarget)
  const [wrap, setWrap] = useState(false)
  const [copied, setCopied] = useState(false)
  const file = useFileContent(repo?.id ?? null, target?.path ?? null, target?.sha ?? null)

  if (!target) return null

  const data = file.data
  const showEditor = data != null && !data.binary && !data.too_large
  const language = languageLabel(target.path)

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
        mode="raw"
        pinnedLabel={target.sha ? `at ${shortSha(target.sha)}` : 'working copy'}
      >
        {showEditor && (
          <>
            <span className="flex-none whitespace-nowrap text-2xs text-muted-foreground">
              {data.line_count.toLocaleString()} {data.line_count === 1 ? 'line' : 'lines'}
              {language ? ` · ${language}` : ''}
            </span>
            <TooltipButton
              onClick={() => setWrap((w) => !w)}
              tooltip={wrap ? 'Stop wrapping long lines' : 'Wrap long lines'}
              aria-pressed={wrap}
              className={cn(
                'flex size-6 flex-none items-center justify-center rounded-[5px] border text-xs',
                wrap
                  ? 'border-border bg-soft text-accent-text'
                  : 'border-border bg-panel2 text-sub hover:border-muted-foreground hover:bg-panel3'
              )}
            >
              <WrapText size={12} />
            </TooltipButton>
            <TooltipButton
              onClick={copy}
              tooltip={copied ? 'Copied' : 'Copy the whole file'}
              className="flex size-6 flex-none items-center justify-center rounded-[5px] border border-border bg-panel2 text-xs text-sub hover:border-muted-foreground hover:bg-panel3"
            >
              {copied ? <Check size={12} className="text-accent-text" /> : <Copy size={12} />}
            </TooltipButton>
          </>
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
            This file is {formatSize(data.size)}, which is too big to open here. Open it in your
            editor instead.
          </div>
        )}
        {showEditor && data.text.length === 0 && (
          <div className="p-4 text-center text-xs text-muted-foreground">This file is empty.</div>
        )}
        {showEditor && data.text.length > 0 && (
          <Suspense
            fallback={
              <div className="p-4 text-center text-xs text-muted-foreground">Opening file…</div>
            }
          >
            <CodeViewer
              key={`${target.path}:${target.sha ?? 'work'}`}
              text={data.text}
              path={target.path}
              wrap={wrap}
              ariaLabel={`Contents of ${target.path}`}
            />
          </Suspense>
        )}
      </div>
    </>
  )
}
