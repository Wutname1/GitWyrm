import { lazy, Suspense, useEffect, useState } from 'react'
import { useQueryClient } from '@tanstack/react-query'
import { Loader2, Pencil, Save, X } from 'lucide-react'
import { toast } from 'sonner'
import { commands } from '@/lib/bindings'
import { unwrap, invalidateOpenspec } from '@/lib/queryKeys'
import { describeError, log } from '@/lib/log'
import { useSpecDraftStore } from '@/stores/specDraftStore'
import { ConfirmDialog } from '@/components/modals/ConfirmDialog'

/**
 * CodeMirror is loaded only when someone actually opens the editor.
 *
 * It is around half a megabyte of JavaScript, and the Desk is mostly read: most
 * sessions never edit a spec file at all. A static import would put that in the
 * main bundle for everyone, so it is split out and paid for on the first Edit
 * click. The chunk is small enough to arrive within the file read that runs
 * alongside it, so in practice nothing waits on it.
 */
const SpecEditor = lazy(async () => ({
  default: (await import('./SpecEditor')).SpecEditor,
}))

/**
 * The Edit control that sits on a spec tab, and the editor it opens.
 *
 * Editing is a mode the user asks for, not the default. A tab that was always an
 * editable text box would let a stray keystroke alter a proposal someone opened
 * to read, and the Desk is mostly read.
 */

/** Opens the editor for one file. Rendered by each tab that owns a file. */
export function EditFileButton({
  onClick,
  label = 'Edit',
}: {
  onClick: () => void
  label?: string
}) {
  return (
    <button
      onClick={onClick}
      className="inline-flex items-center gap-1.5 rounded border border-border px-2 py-1 text-2xs font-semibold text-sub transition-colors hover:border-primary hover:text-foreground"
    >
      <Pencil size={11} />
      {label}
    </button>
  )
}

export function SpecFileEditor({
  repoId,
  changeId,
  file,
  onClose,
}: {
  repoId: string
  changeId: string
  /** Path relative to the change folder: `proposal.md`, `specs/core/spec.md`. */
  file: string
  onClose: () => void
}) {
  const qc = useQueryClient()
  const draft = useSpecDraftStore((s) => s.drafts[`${repoId} ${changeId} ${file}`])
  const { open, edit, discard } = useSpecDraftStore()
  const [loading, setLoading] = useState(!draft)
  const [saving, setSaving] = useState(false)
  const [loadError, setLoadError] = useState<string | null>(null)
  const [confirmDiscard, setConfirmDiscard] = useState(false)

  const dirty = draft != null && draft.text !== draft.original

  // Read the file once, unless a draft for it is already in hand -- in which
  // case the unsaved text is what the user expects to see, not what is on disk.
  useEffect(() => {
    // Read through the store rather than the subscribed `draft`: this effect
    // does not depend on `draft` (it must not refetch on every keystroke), so
    // that binding is from the first render and can be stale by the time a
    // second mount runs.
    if (useSpecDraftStore.getState().get(repoId, changeId, file)) {
      setLoading(false)
      return
    }
    let cancelled = false
    setLoading(true)
    void (async () => {
      try {
        const body = unwrap(await commands.openspecReadFile(repoId, changeId, file))
        // Left in on purpose. "The editor opened empty" has several possible
        // causes -- a short read, a draft that never landed, a mount race --
        // and they are indistinguishable from the outside. This names which
        // one, in one line, without needing a reproduction.
        log.info(
          `spec editor: read ${file} of ${changeId} (${body.length} chars)`
        )
        // The store call is deliberately NOT guarded by `cancelled`. StrictMode
        // and Fast Refresh mount this effect twice, cancelling the first pass
        // mid-flight; if a cancelled read dropped its result, the editor could
        // end up with no draft at all and render empty. Seeding the store is
        // idempotent -- `open` keeps an existing draft -- so doing it from a
        // superseded pass is harmless, while skipping it is not.
        open(repoId, changeId, file, body)
        if (cancelled) return
        setLoadError(null)
      } catch (e) {
        if (!cancelled) setLoadError(describeError(e))
      } finally {
        if (!cancelled) setLoading(false)
      }
    })()
    return () => {
      cancelled = true
    }
    // `draft` is deliberately absent: re-running on every keystroke would
    // refetch the file out from under the editor.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [repoId, changeId, file])

  const save = async () => {
    if (!draft || saving) return
    setSaving(true)
    try {
      unwrap(await commands.openspecWriteFile(repoId, changeId, file, draft.text))
      // The draft is only dropped once the write has actually landed, so a
      // failure below leaves the user's text exactly where it was.
      discard(repoId, changeId, file)
      invalidateOpenspec(qc, repoId)
      toast.success(`Saved ${file}.`)
      onClose()
    } catch (e) {
      toast.error(`${file} could not be saved.`, { description: describeError(e) })
    } finally {
      setSaving(false)
    }
  }

  const close = () => {
    if (dirty) {
      setConfirmDiscard(true)
      return
    }
    discard(repoId, changeId, file)
    onClose()
  }

  return (
    <div className="flex min-h-0 flex-col rounded-lg border border-border">
      <header className="flex flex-none items-center justify-between gap-3 border-b border-border bg-panel px-3 py-1.5">
        <span className="min-w-0 overflow-hidden text-ellipsis whitespace-nowrap font-mono text-2xs text-sub">
          {file}
          {dirty && (
            <span className="ml-2 text-[var(--gw-amber)]" title="Not saved yet">
              . unsaved
            </span>
          )}
        </span>
        <div className="flex flex-none items-center gap-1.5">
          <button
            onClick={close}
            disabled={saving}
            className="inline-flex items-center gap-1 rounded px-2 py-1 text-2xs font-semibold text-sub transition-colors hover:text-foreground disabled:opacity-50"
          >
            <X size={11} />
            Cancel
          </button>
          <button
            onClick={() => void save()}
            disabled={!dirty || saving || loading}
            className="inline-flex items-center gap-1 rounded bg-primary px-2 py-1 text-2xs font-semibold text-accent-fg transition-opacity hover:opacity-90 disabled:opacity-40"
          >
            {saving ? <Loader2 size={11} className="animate-spin" /> : <Save size={11} />}
            {saving ? 'Saving...' : 'Save'}
          </button>
        </div>
      </header>

      <div className="min-h-0 flex-1">
        {loading ? (
          <p className="px-3 py-4 text-2xs text-muted-foreground">Opening {file}...</p>
        ) : loadError ? (
          <p className="px-3 py-4 text-2xs text-[var(--gw-red)]">{loadError}</p>
        ) : (
          <div className="h-[min(60vh,520px)]">
            <Suspense
              fallback={
                <p className="px-3 py-4 text-2xs text-muted-foreground">Opening the editor...</p>
              }
            >
              <SpecEditor
                value={draft?.text ?? ''}
                onChange={(next) => edit(repoId, changeId, file, next)}
                onSave={() => void save()}
                ariaLabel={`Edit ${file}`}
              />
            </Suspense>
          </div>
        )}
      </div>

      <ConfirmDialog
        open={confirmDiscard}
        onOpenChange={setConfirmDiscard}
        title={`Throw away your changes to ${file}?`}
        description="What you typed here has not been saved. Closing now loses it."
        confirmLabel="Throw them away"
        destructive
        onConfirm={() => {
          discard(repoId, changeId, file)
          setConfirmDiscard(false)
          onClose()
        }}
      />
    </div>
  )
}
