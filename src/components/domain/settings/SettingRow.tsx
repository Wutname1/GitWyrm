import { useState } from 'react'
import { Folder } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { normalizePath } from '@/lib/paths'

export function SettingRow({
  label,
  hint,
  children,
}: {
  label: string
  hint?: string
  children: React.ReactNode
}) {
  return (
    <div className="flex items-start gap-6 py-3">
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
