import { useMemo, useState } from 'react'
import { Archive, Search } from 'lucide-react'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import { Input } from '@/components/ui/input'
import { useOpenspecArchived } from '@/hooks/useOpenspec'

/**
 * The archived changes, searchable by name.
 *
 * Only their ids are read: an archived change's requirements have already been
 * folded into the specs library, so the useful question here is "did we do that,
 * and what was it called?" rather than re-reading a finished plan. Opening one is
 * a job for the specs library once that view exists.
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
  // Only fetched while open -- it walks changes/archive on disk.
  const archived = useOpenspecArchived(repoId, open)
  const ids = archived.data ?? []

  const matches = useMemo(() => {
    const q = query.trim().toLowerCase()
    if (!q) return ids
    return ids.filter((id) => id.toLowerCase().includes(q))
  }, [ids, query])

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-lg">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <Archive size={14} strokeWidth={2.2} className="text-accent-text" />
            Archived changes
          </DialogTitle>
          <DialogDescription>
            Work that shipped. Each of these had its requirements folded into your specs.
          </DialogDescription>
        </DialogHeader>

        <div className="flex items-center gap-2 rounded-md border border-border bg-panel2 px-2.5 py-1.5">
          <Search size={13} strokeWidth={2} className="flex-none text-muted-foreground" />
          <Input
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder="Search archived changes"
            className="h-auto border-0 bg-transparent p-0 text-xs focus-visible:ring-0"
          />
          {ids.length > 0 && (
            <span className="flex-none font-mono text-2xs text-muted-foreground">
              {matches.length}/{ids.length}
            </span>
          )}
        </div>

        <div className="max-h-72 overflow-y-auto">
          {archived.isLoading ? (
            <p className="px-1 py-2 text-xs text-muted-foreground">Reading the archive…</p>
          ) : ids.length === 0 ? (
            <p className="px-1 py-2 text-xs text-muted-foreground">
              Nothing archived yet. A change lands here once every task is done and you
              archive it.
            </p>
          ) : matches.length === 0 ? (
            <p className="px-1 py-2 text-xs text-muted-foreground">
              No archived change matches "{query.trim()}".
            </p>
          ) : (
            <ul className="flex flex-col">
              {matches.map((id) => (
                <li
                  key={id}
                  className="border-b border-border/60 px-1 py-2 font-mono text-xs text-sub last:border-b-0"
                >
                  {id}
                </li>
              ))}
            </ul>
          )}
        </div>
      </DialogContent>
    </Dialog>
  )
}
