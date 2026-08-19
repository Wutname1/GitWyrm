import { useEffect, useMemo, useState } from 'react'
import { Archive, Search } from 'lucide-react'
import { cn } from '@/lib/utils'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import { Input } from '@/components/ui/input'
import { formatRelativeTime } from '@/lib/gitDisplay'
import { useOpenspecArchived } from '@/hooks/useOpenspec'
import { ArchivedChangeDetail } from './ArchivedChangeDetail'

/**
 * The archive: every change that shipped, searchable, and readable.
 *
 * Archived work is the record of what was decided and why, so the list is a way
 * in rather than the whole answer -- picking a row opens the change itself, the
 * same proposal, deltas and tasks it had on the day it was archived. Nothing
 * here writes: an archived change has already been folded into the specs
 * library, and editing it would rewrite finished work.
 */
export function ArchivedChangesDialog({
  repoId,
  open,
  onOpenChange,
}: {
  repoId: string
  open: boolean
  onOpenChange: (open: boolean) => void
}) {
  const [query, setQuery] = useState('')
  const [selectedId, setSelectedId] = useState<string | null>(null)
  // Only fetched while open -- it walks changes/archive on disk.
  const archived = useOpenspecArchived(repoId, open)
  const changes = useMemo(() => archived.data ?? [], [archived.data])

  const matches = useMemo(() => {
    const q = query.trim().toLowerCase()
    if (!q) return changes
    return changes.filter(
      (c) => c.id.toLowerCase().includes(q) || c.title.toLowerCase().includes(q)
    )
  }, [changes, query])

  // Always have something open once there is something to open: an archive that
  // greets you with an empty right-hand pane makes the list look like the whole
  // feature. Filtering to a set that excludes the open change moves to the first
  // match rather than leaving a selection you can no longer see.
  useEffect(() => {
    if (matches.length === 0) return
    if (selectedId && matches.some((c) => c.id === selectedId)) return
    setSelectedId(matches[0].id)
  }, [matches, selectedId])

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="flex h-[70vh] flex-col gap-0 p-0 sm:max-w-4xl">
        <DialogHeader className="flex-none border-b border-border px-4 pb-3 pt-4">
          <DialogTitle className="flex items-center gap-2">
            <Archive size={14} strokeWidth={2.2} className="text-accent-text" />
            Archived changes
          </DialogTitle>
          <DialogDescription>
            Work that shipped. Each of these had its requirements folded into your specs.
          </DialogDescription>
        </DialogHeader>

        <div className="grid min-h-0 flex-1 grid-cols-[minmax(0,15rem)_1fr]">
          <div className="flex min-h-0 flex-col border-r border-border">
            <div className="m-3 flex flex-none items-center gap-2 rounded-md border border-border bg-panel2 px-2.5 py-1.5">
              <Search size={13} strokeWidth={2} className="flex-none text-muted-foreground" />
              <Input
                value={query}
                onChange={(e) => setQuery(e.target.value)}
                placeholder="Search the archive"
                className="h-auto border-0 bg-transparent p-0 text-xs focus-visible:ring-0"
              />
              {changes.length > 0 && (
                <span className="flex-none font-mono text-2xs text-muted-foreground">
                  {matches.length}/{changes.length}
                </span>
              )}
            </div>

            <div className="min-h-0 flex-1 overflow-y-auto px-2 pb-3">
              {archived.isLoading ? (
                <p className="px-2 py-2 text-xs text-muted-foreground">Reading the archive…</p>
              ) : changes.length === 0 ? (
                <p className="px-2 py-2 text-xs text-muted-foreground">
                  Nothing archived yet. A change lands here once every task is done and you
                  archive it.
                </p>
              ) : matches.length === 0 ? (
                <p className="px-2 py-2 text-xs text-muted-foreground">
                  Nothing in the archive matches "{query.trim()}".
                </p>
              ) : (
                <ul className="flex flex-col gap-0.5">
                  {matches.map((change) => (
                    <li key={change.id}>
                      <button
                        type="button"
                        onClick={() => setSelectedId(change.id)}
                        aria-pressed={change.id === selectedId}
                        className={cn(
                          'w-full rounded px-2 py-1.5 text-left transition-colors',
                          change.id === selectedId
                            ? 'bg-soft text-foreground'
                            : 'text-sub hover:bg-panel2'
                        )}
                      >
                        <span className="block truncate text-xs font-medium">
                          {change.title}
                        </span>
                        <span className="mt-0.5 block truncate text-2xs text-muted-foreground">
                          {change.total > 0 && `${change.total} tasks · `}
                          {formatRelativeTime(change.updated)}
                        </span>
                      </button>
                    </li>
                  ))}
                </ul>
              )}
            </div>
          </div>

          <div className="min-h-0 overflow-hidden">
            {selectedId ? (
              <ArchivedChangeDetail key={selectedId} repoId={repoId} changeId={selectedId} />
            ) : (
              <p className="px-4 py-3 text-xs text-muted-foreground">
                Pick a change on the left to read it.
              </p>
            )}
          </div>
        </div>
      </DialogContent>
    </Dialog>
  )
}
