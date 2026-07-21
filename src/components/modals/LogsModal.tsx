import { useEffect, useMemo, useState } from 'react'
import { Copy, RefreshCw } from 'lucide-react'
import { toast } from 'sonner'
import { copyToClipboard } from '@/lib/clipboard'
import { cn } from '@/lib/utils'
import { Button } from '@/components/ui/button'
import { PendingIndicator } from '@/components/ui/pending-indicator'
import { Dialog, DialogContent, DialogHeader, DialogTitle } from '@/components/ui/dialog'
import { Input } from '@/components/ui/input'
import { commands } from '@/lib/bindings'
import { unwrap } from '@/lib/queryKeys'
import { ClearLogsButton, OpenLogsFolderButton, useClearLogs } from '@/components/domain/settings/LogActions'

type Level = 'all' | 'warn' | 'error'

const LEVEL_RE = /\[(TRACE|DEBUG|INFO|WARN|ERROR)\]/

function lineLevel(line: string): string | null {
  return LEVEL_RE.exec(line)?.[1] ?? null
}

function LogLine({ line }: { line: string }) {
  const level = lineLevel(line)
  return (
    <div
      className={cn(
        'whitespace-pre-wrap break-all border-l-2 border-transparent px-2 py-px font-mono text-2xs leading-[1.5]',
        level === 'ERROR' && 'border-removed bg-removed/5 text-removed',
        level === 'WARN' && 'border-amber-400 text-amber-300',
        level === 'DEBUG' && 'text-muted-foreground',
        (level === 'INFO' || level === null) && 'text-sub'
      )}
    >
      {line}
    </div>
  )
}

export function LogsModal({ open, onClose }: { open: boolean; onClose: () => void }) {
  const [raw, setRaw] = useState('')
  const [loading, setLoading] = useState(false)
  const [level, setLevel] = useState<Level>('all')
  const [search, setSearch] = useState('')
  const { clearing, clearLogs } = useClearLogs(() => setRaw(''))

  const load = async () => {
    setLoading(true)
    try {
      setRaw(unwrap(await commands.readLog()))
    } catch (e) {
      toast.error(`Could not read log: ${(e as Error).message}`)
    } finally {
      setLoading(false)
    }
  }

  useEffect(() => {
    if (open) load()
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open])

  // Group continuation lines (stack traces etc.) with the log event above them,
  // so level filtering keeps multi-line events intact.
  const events = useMemo(() => {
    const lines = raw.split(/\r?\n/)
    const out: { level: string | null; text: string }[] = []
    for (const line of lines) {
      if (!line.trim()) continue
      const lvl = lineLevel(line)
      if (lvl === null && out.length > 0) {
        out[out.length - 1].text += `\n${line}`
      } else {
        out.push({ level: lvl, text: line })
      }
    }
    return out
  }, [raw])

  const filtered = useMemo(() => {
    const q = search.toLowerCase()
    return events.filter((e) => {
      if (level === 'error' && e.level !== 'ERROR') return false
      if (level === 'warn' && e.level !== 'ERROR' && e.level !== 'WARN') return false
      if (q && !e.text.toLowerCase().includes(q)) return false
      return true
    })
  }, [events, level, search])

  const errorCount = useMemo(() => events.filter((e) => e.level === 'ERROR').length, [events])

  const copyView = () =>
    void copyToClipboard(filtered.map((e) => e.text).join('\n'), 'Copied to clipboard')

  return (
    <Dialog open={open} onOpenChange={(o) => !o && onClose()}>
      <DialogContent className="flex h-[70vh] flex-col gap-0 p-0 sm:max-w-3xl" aria-describedby={undefined}>
        <DialogHeader className="flex-none border-b border-border px-4 pb-3 pt-4">
          <DialogTitle className="flex items-center gap-2 text-sm">
            Application log
            {errorCount > 0 && (
              <span className="rounded bg-removed/15 px-1.5 py-0.5 text-2xs font-semibold text-removed">
                {errorCount} error{errorCount === 1 ? '' : 's'}
              </span>
            )}
          </DialogTitle>
          <div className="mt-2 flex items-center gap-2">
            <div className="flex rounded-md border border-border p-0.5">
              {(
                [
                  ['all', 'All'],
                  ['warn', 'Warnings+'],
                  ['error', 'Errors'],
                ] as [Level, string][]
              ).map(([key, label]) => (
                <button
                  key={key}
                  onClick={() => setLevel(key)}
                  className={cn(
                    'rounded px-2.5 py-1 text-2xs font-medium',
                    level === key ? 'bg-panel3 text-foreground' : 'text-sub hover:text-foreground'
                  )}
                >
                  {label}
                </button>
              ))}
            </div>
            <Input
              value={search}
              onChange={(e) => setSearch(e.target.value)}
              placeholder="Search log…"
              className="h-7 flex-1 bg-background text-xs"
            />
            <Button variant="secondary" size="sm" className="h-7 gap-1.5 text-xs" onClick={load} disabled={loading}>
              <RefreshCw size={12} className={cn(loading && 'animate-spin')} />
              Refresh
            </Button>
          </div>
        </DialogHeader>

        <div className="min-h-0 flex-1 overflow-y-auto bg-background py-1">
          {filtered.length === 0 && (
            <div className="px-4 py-8 text-center text-xs text-muted-foreground">
              {events.length === 0 ? 'Log is empty.' : 'No entries match the current filter.'}
            </div>
          )}
          {filtered.map((e, i) => (
            <LogLine key={i} line={e.text} />
          ))}
        </div>

        <div className="flex flex-none items-center gap-2 border-t border-border px-4 py-2.5">
          <OpenLogsFolderButton />
          <Button variant="secondary" size="sm" className="h-7 gap-1.5 text-xs" onClick={copyView}>
            <Copy size={12} />
            Copy view
          </Button>
          <div className="flex-1" />
          <ClearLogsButton clearing={clearing} onClear={clearLogs} />
        </div>
      </DialogContent>
    </Dialog>
  )
}
