import { ArrowLeft } from 'lucide-react'
import { cn } from '@/lib/utils'
import type { SettingsSection } from '@/stores/uiStore'
import { useUiStore } from '@/stores/uiStore'

type NavItem = { key: SettingsSection; label: string }

const APP_ITEMS: NavItem[] = [
  { key: 'general', label: 'General' },
  { key: 'ai', label: 'AI' },
  { key: 'appearance', label: 'Appearance' },
  { key: 'logs', label: 'Logs' },
  { key: 'about', label: 'About' },
]

const REPO_ITEMS: NavItem[] = [
  { key: 'repository', label: 'Repository' },
  { key: 'tags', label: 'Tags' },
]

export function SettingsNav({
  active,
  onSelect,
}: {
  active: SettingsSection
  onSelect: (section: SettingsSection) => void
}) {
  const showGraph = useUiStore((s) => s.showGraph)

  const renderItem = (item: NavItem) => (
    <button
      key={item.key}
      onClick={() => onSelect(item.key)}
      className={cn(
        'block w-full rounded-md px-3 py-1.5 text-left text-xs font-medium text-sub hover:bg-panel2 hover:text-foreground',
        active === item.key && 'bg-panel2 text-foreground'
      )}
    >
      {item.label}
    </button>
  )

  return (
    <div className="flex w-56 flex-none flex-col border-r border-border bg-panel">
      <div className="flex h-10 flex-none items-center border-b border-border px-2">
        <button
          onClick={showGraph}
          className="flex w-full items-center gap-2 rounded-md border border-primary/60 bg-primary/5 px-3 py-1.5 text-left text-xs font-semibold text-foreground transition-colors hover:border-primary hover:bg-primary/10"
        >
          <ArrowLeft size={14} className="text-accent-text" />
          Exit Settings
        </button>
      </div>

      <div className="min-h-0 flex-1 overflow-y-auto p-2">
        <div className="px-3 pb-1 pt-2 text-[10px] font-bold uppercase tracking-[.08em] text-muted-foreground">
          Application
        </div>
        {APP_ITEMS.map(renderItem)}

        <div className="mx-3 my-2 border-t border-border" />

        <div className="px-3 pb-1 pt-1 text-[10px] font-bold uppercase tracking-[.08em] text-muted-foreground">
          This repository
        </div>
        {REPO_ITEMS.map(renderItem)}
      </div>
    </div>
  )
}
