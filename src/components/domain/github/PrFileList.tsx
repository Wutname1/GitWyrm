import { useMemo, useState } from 'react'
import { ChevronDown, ChevronRight, FileDiff } from 'lucide-react'
import type { PrFile } from '@/lib/bindings'
import { DiffLineRow } from '@/components/domain/diff/DiffLineRow'
import { computeWordSpans } from '@/lib/wordDiff'
import { PendingIndicator } from '@/components/ui/pending-indicator'
import { cn } from '@/lib/utils'

/**
 * How the host described what happened to a file, in words a reader who does not
 * think in git terms can act on. "removed" is the host's spelling; "deleted" is
 * what people say.
 */
const STATUS_LABEL: Record<string, string> = {
  added: 'new file',
  removed: 'deleted',
  modified: 'changed',
  renamed: 'renamed',
  copied: 'copied',
  changed: 'changed',
  unchanged: 'unchanged',
}

function StatusBadge({ status }: { status: string }) {
  const label = STATUS_LABEL[status] ?? status
  return (
    <span
      className={cn(
        'flex-none rounded-full border px-[7px] py-px text-2xs font-semibold',
        status === 'added'
          ? 'border-added/40 bg-added/10 text-added'
          : status === 'removed'
            ? 'border-removed/40 bg-removed/10 text-removed'
            : 'border-border bg-panel3 text-sub'
      )}
    >
      {label}
    </span>
  )
}

/** One file: a clickable header that expands to the diff underneath. */
function FileRow({ file, defaultOpen }: { file: PrFile; defaultOpen: boolean }) {
  const [open, setOpen] = useState(defaultOpen)
  const lines = file.diff?.lines ?? []

  // Only computed for files the reader has actually opened; a large pull request
  // would otherwise pay for every file's word diff up front.
  const wordSpans = useMemo(() => (open ? computeWordSpans(lines) : null), [open, lines])

  return (
    <div className="border-b border-border/60 last:border-b-0">
      <button
        type="button"
        onClick={() => setOpen((v) => !v)}
        className="flex w-full items-center gap-2 px-3.5 py-2 text-left hover:bg-panel3/60"
      >
        {open ? (
          <ChevronDown size={13} className="flex-none text-muted-foreground" />
        ) : (
          <ChevronRight size={13} className="flex-none text-muted-foreground" />
        )}
        <span className="truncate font-mono text-xs text-foreground">{file.path}</span>
        <StatusBadge status={file.status} />
        {file.old_path && (
          <span className="truncate font-mono text-2xs text-muted-foreground">
            was {file.old_path}
          </span>
        )}
        <span className="ml-auto flex flex-none items-center gap-2 font-mono text-2xs">
          <span className="text-added">+{file.additions}</span>
          <span className="text-removed">−{file.deletions}</span>
        </span>
      </button>

      {open && (
        <div className="border-t border-border/60 bg-background">
          {lines.length > 0 ? (
            <div className="overflow-x-auto py-1 font-mono text-xs leading-[1.5]">
              {lines.map((line, i) => (
                <DiffLineRow
                  key={`${line.hunk_index}:${line.sign}:${line.old_no ?? ''}:${line.new_no ?? ''}:${i}`}
                  line={line}
                  wordSpans={wordSpans?.get(i)}
                />
              ))}
            </div>
          ) : (
            // Binary files and ones the host declined to inline. Saying which is
            // not possible from the payload, so the message covers both without
            // guessing.
            <p className="px-3.5 py-3 text-xs text-muted-foreground">
              No preview for this file. Open the pull request in your browser to see it.
            </p>
          )}
        </div>
      )}
    </div>
  )
}

/**
 * The files a pull request changes.
 *
 * Files start collapsed once there are more than a handful, so a big pull
 * request opens as a readable list rather than a wall of diff.
 */
export function PrFileList({
  files,
  loading,
  error,
}: {
  files: PrFile[] | undefined
  loading: boolean
  error: boolean
}) {
  // Small changes are almost always read in full, so opening them saves a click.
  // Past that the list itself is the useful view.
  const expandByDefault = (files?.length ?? 0) <= 3

  const totals = useMemo(
    () =>
      (files ?? []).reduce(
        (acc, f) => ({ add: acc.add + f.additions, del: acc.del + f.deletions }),
        { add: 0, del: 0 }
      ),
    [files]
  )

  return (
    <section className="mt-4 overflow-hidden rounded-md border border-border bg-panel2/70">
      <div className="flex items-center gap-2 border-b border-border px-3.5 py-2.5 text-2xs font-bold text-foreground">
        <FileDiff size={13} className="text-muted-foreground" />
        Files changed
        {files && files.length > 0 && (
          <span className="ml-auto flex items-center gap-2 font-normal">
            <span className="text-muted-foreground">
              {files.length === 1 ? '1 file' : `${files.length} files`}
            </span>
            <span className="font-mono text-added">+{totals.add}</span>
            <span className="font-mono text-removed">−{totals.del}</span>
          </span>
        )}
      </div>

      {loading && (
        <div className="flex items-center gap-2 px-3.5 py-3 text-xs text-muted-foreground">
          <PendingIndicator /> Loading the changes…
        </div>
      )}
      {error && !loading && (
        <p className="px-3.5 py-3 text-xs text-removed">
          Could not load the changed files. Try Refresh.
        </p>
      )}
      {!loading && !error && files?.length === 0 && (
        <p className="px-3.5 py-3 text-xs text-muted-foreground">
          This pull request does not change any files.
        </p>
      )}
      {!loading &&
        files?.map((f) => (
          <FileRow key={f.path} file={f} defaultOpen={expandByDefault} />
        ))}

      {/* GitHub serves at most 300 files and we read one page of 100. Saying so
          keeps a truncated list from reading as the whole change. */}
      {(files?.length ?? 0) >= 100 && (
        <p className="border-t border-border/60 px-3.5 py-2.5 text-2xs text-muted-foreground">
          Showing the first {files!.length} files. Open the pull request in your browser to see the
          rest.
        </p>
      )}
    </section>
  )
}
