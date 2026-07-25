import { useEffect, useRef, useState } from 'react'
import { Check, FileSearch, Loader2, Package, X } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'

/** Where a resolved tool came from. Mirrors ToolSource in the Rust bindings. */
export type ToolSourceValue = 'configured' | 'system' | 'bundled' | 'missing'

export type ToolState =
  | { state: 'checking' }
  | { state: 'ok'; version: string; source: ToolSourceValue }
  | { state: 'error'; message: string }

/**
 * A path input plus a live status line for one bundled-or-system tool (git or
 * gpg). Leaving the box empty is the normal case: GitWyrm finds the tool on
 * its own and the status line says which one it landed on, so the user can
 * tell "my install" from "the one that came with GitWyrm" at a glance.
 */
export function ToolStatusRow({
  value,
  onCommit,
  onBrowse,
  status,
  browseLabel,
}: {
  value: string
  onCommit: (next: string) => void
  onBrowse: () => void
  status: ToolState
  browseLabel: string
}) {
  const [draft, setDraft] = useState(value)

  // Keep the local draft in step if the value changes elsewhere (e.g. a reset).
  useEffect(() => {
    setDraft(value)
  }, [value])

  const commit = (raw: string) => {
    const next = raw.trim()
    setDraft(next)
    if (next !== value) onCommit(next)
  }

  return (
    <div className="grid gap-1.5">
      <div className="flex gap-2">
        <Input
          value={draft}
          onChange={(e) => setDraft(e.target.value)}
          onBlur={() => commit(draft)}
          onKeyDown={(e) => e.key === 'Enter' && commit(draft)}
          placeholder="Found automatically"
          className="h-8 bg-background font-mono text-xs"
          aria-label={browseLabel}
        />
        <Button
          variant="secondary"
          size="sm"
          className="h-8 flex-none"
          tooltip={browseLabel}
          onClick={onBrowse}
        >
          <FileSearch size={13} />
          Browse
        </Button>
      </div>
      <ToolStatusLine status={status} />
    </div>
  )
}

/** Plain-language note about which copy is in use (Rule #2). */
const SOURCE_NOTES: Record<ToolSourceValue, string> = {
  configured: 'the one you picked',
  system: 'already on this computer',
  bundled: 'included with GitWyrm',
  missing: '',
}

export function ToolStatusLine({ status }: { status: ToolState }) {
  if (status.state === 'checking') {
    return (
      <div className="flex items-center gap-1.5 text-2xs text-muted-foreground">
        <Loader2 size={12} className="animate-spin" />
        Checking...
      </div>
    )
  }

  if (status.state === 'error') {
    return (
      <div className="flex items-start gap-1.5 text-2xs text-red-400">
        <X size={12} className="mt-0.5 flex-none" />
        <span>{status.message}</span>
      </div>
    )
  }

  const note = SOURCE_NOTES[status.source]
  return (
    <div className="flex items-center gap-1.5 text-2xs text-emerald-500">
      {status.source === 'bundled' ? (
        <Package size={12} className="flex-none" />
      ) : (
        <Check size={12} className="flex-none" />
      )}
      <span className="truncate">
        {status.version}
        {note && <span className="text-muted-foreground"> ({note})</span>}
      </span>
    </div>
  )
}

/** Shared hook: re-run a check whenever its input changes, ignoring stale results. */
export function useToolCheck(run: () => Promise<ToolState>, deps: unknown[]) {
  const [status, setStatus] = useState<ToolState>({ state: 'checking' })
  const runId = useRef(0)

  useEffect(() => {
    const id = ++runId.current
    setStatus({ state: 'checking' })
    run()
      .then((next) => {
        // A slower earlier check must not overwrite a newer answer.
        if (id === runId.current) setStatus(next)
      })
      .catch((e: unknown) => {
        if (id === runId.current) {
          setStatus({ state: 'error', message: e instanceof Error ? e.message : String(e) })
        }
      })
    // The caller owns what invalidates the check.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, deps)

  return status
}
