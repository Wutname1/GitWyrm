import { useEffect, useMemo, useState } from 'react'
import { Copy, RefreshCw } from 'lucide-react'
import { toast } from 'sonner'
import { copyToClipboard } from '@/lib/clipboard'
import { cn } from '@/lib/utils'
import { Button } from '@/components/ui/button'
import { PendingIndicator } from '@/components/ui/pending-indicator'
import { Dialog, DialogContent, DialogHeader, DialogTitle } from '@/components/ui/dialog'
import { Input } from '@/components/ui/input'
import { commands, type LogFile } from '@/lib/bindings'
import { unwrap } from '@/lib/queryKeys'
import { ClearLogsButton, OpenLogsFolderButton, useClearLogs } from '@/components/domain/settings/LogActions'

type Level = 'all' | 'warn' | 'error'

const LEVEL_RE = /\[(TRACE|DEBUG|INFO|WARN|ERROR)\]/

const SELECT_CLASS =
  'h-7 rounded-md border border-input bg-background px-2 text-xs text-foreground outline-none focus:border-ring'

/** `gitwyrm-2026-08-13.log`, or `gitwyrm-2026-08-13-2.log` once a day fills up. */
const DAILY_RE = /^gitwyrm-(\d{4})-(\d{2})-(\d{2})(?:-(\d+))?\.log$/

/**
 * A log file named the way someone would say it: "Today", "Yesterday", or the
 * date. Files that are not one of the daily ones -- the panic log, and what an
 * older GitWyrm left behind -- keep their file name, since there is nothing
 * friendlier to call them.
 */
function logFileLabel(file: LogFile): string {
  const match = DAILY_RE.exec(file.name)
  if (!match) return file.name

  const [, year, month, day, part] = match
  const date = new Date(Number(year), Number(month) - 1, Number(day))
  const midnight = new Date()
  midnight.setHours(0, 0, 0, 0)
  const daysAgo = Math.round((midnight.getTime() - date.getTime()) / 86_400_000)

  let label = date.toLocaleDateString(undefined, { month: 'short', day: 'numeric', year: 'numeric' })
  if (daysAgo === 0) label = 'Today'
  else if (daysAgo === 1) label = 'Yesterday'
  if (part) label += ` (part ${part})`
  return label
}

function formatSize(bytes: number): string {
  if (bytes < 1_000) return `${bytes} B`
  if (bytes < 1_000_000) return `${Math.round(bytes / 1_000)} KB`
  return `${(bytes / 1_000_000).toFixed(1)} MB`
}

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
  const [files, setFiles] = useState<LogFile[]>([])
  const [selected, setSelected] = useState('')
  const [raw, setRaw] = useState('')
  const [loading, setLoading] = useState(false)
  const [level, setLevel] = useState<Level>('all')
  const [search, setSearch] = useState('')
  const { clearing, clearLogs } = useClearLogs(() => void load())

  /**
   * Read the list of days and one day's contents. `name` picks the file; without
   * it the modal keeps showing whichever day it was already on, falling back to
   * the one being written to now.
   */
  const load = async (name?: string) => {
    setLoading(true)
    try {
      const listed = unwrap(await commands.listLogs())
      setFiles(listed)
      const wanted = name ?? selected
      const file =
        listed.find((f) => f.name === wanted) ?? listed.find((f) => f.active) ?? listed[0]
      setSelected(file?.name ?? '')
      setRaw(file ? unwrap(await commands.readLogFile(file.name)) : '')
    } catch (e) {
      toast.error(`Could not read log: ${(e as Error).message}`)
    } finally {
      setLoading(false)
    }
  }

  useEffect(() => {
    if (open) void load()
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
            <select
              className={SELECT_CLASS}
              value={selected}
              onChange={(e) => void load(e.target.value)}
              disabled={files.length === 0}
              aria-label="Log file"
            >
              {files.length === 0 && <option value="">No logs yet</option>}
              {files.map((file) => (
                <option key={file.name} value={file.name}>
                  {logFileLabel(file)} - {formatSize(file.size)}
                </option>
              ))}
            </select>
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
            <Button
              variant="secondary"
              size="sm"
              className="h-7 gap-1.5 text-xs"
              onClick={() => void load()}
              disabled={loading}
            >
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
