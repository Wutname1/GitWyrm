import { useEffect, useRef, useState } from 'react'
import { Folder } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { cn } from '@/lib/utils'
import { normalizePath } from '@/lib/paths'
import { useUiStore } from '@/stores/uiStore'

/**
 * Scrolls the row into view and flashes it when settings search reveals it.
 * `searchId` matches an entry in the settings search index.
 */
export function useRevealHighlight(searchId: string | undefined) {
  const ref = useRef<HTMLDivElement>(null)
  const reveal = useUiStore((s) => s.revealSetting)
  const [flash, setFlash] = useState(false)

  const nonce = searchId && reveal?.id === searchId ? reveal.nonce : null

  useEffect(() => {
    if (nonce === null) return
    ref.current?.scrollIntoView({ block: 'center', behavior: 'smooth' })
    setFlash(true)
    const timer = window.setTimeout(() => setFlash(false), 1600)
    return () => window.clearTimeout(timer)
  }, [nonce])

  return { ref, flash }
}

/** Row shell classes, shared with the few blocks that lay out their own row. */
export function settingRowClass(flash: boolean) {
  return cn(
    '-mx-3 flex scroll-mt-6 items-start gap-6 rounded-md px-3 py-3 transition-colors duration-500',
    flash && 'bg-primary/10 ring-1 ring-primary/50'
  )
}

export function SettingRow({
  label,
  hint,
  searchId,
  children,
}: {
  label: string
  hint?: string
  /** Id from the settings search index, so search results can jump here. */
  searchId?: string
  children: React.ReactNode
}) {
  const { ref, flash } = useRevealHighlight(searchId)

  return (
    <div ref={ref} className={settingRowClass(flash)}>
      <div className="w-52 flex-none">
        <div className="text-xs font-semibold text-foreground">{label}</div>
        {hint && <div className="mt-0.5 text-2xs text-muted-foreground">{hint}</div>}
      </div>
      <div className="min-w-0 flex-1">{children}</div>
    </div>
  )
}

/** Path input bound to a store setting: normalizes on blur, Browse via native dialog. */
export function FolderSetting({
  value,
  placeholder,
  onCommit,
}: {
  value: string | null
  placeholder: string
  onCommit: (path: string | null) => void
}) {
  const [draft, setDraft] = useState(value ?? '')

  const commit = (raw: string) => {
    const trimmed = raw.trim()
    const normalized = trimmed ? normalizePath(trimmed) : null
    setDraft(normalized ?? '')
    onCommit(normalized)
  }

  return (
    <div className="flex gap-2">
      <Input
        value={draft}
        onChange={(e) => setDraft(e.target.value)}
        onBlur={() => commit(draft)}
        onKeyDown={(e) => e.key === 'Enter' && commit(draft)}
        placeholder={placeholder}
        className="h-8 bg-background font-mono text-xs"
      />
      <Button
        variant="secondary"
        size="sm"
        className="h-8 flex-none"
        tooltip="Browse for folder"
        onClick={async () => {
          const { open: openDialog } = await import('@tauri-apps/plugin-dialog')
          const dir = await openDialog({ directory: true, title: 'Select folder' })
          if (typeof dir === 'string') commit(dir)
        }}
      >
        <Folder size={13} />
      </Button>
    </div>
  )
}
