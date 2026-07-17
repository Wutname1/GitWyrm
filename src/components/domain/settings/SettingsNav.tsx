import { cn } from '@/lib/utils'
import type { SettingsSection } from '@/stores/uiStore'

const ITEMS: { key: SettingsSection; label: string }[] = [
  { key: 'general', label: 'General' },
  { key: 'ai', label: 'AI' },
  { key: 'appearance', label: 'Appearance' },
  { key: 'logs', label: 'Logs' },
  { key: 'about', label: 'About' },
]

export function SettingsNav({
  active,
  onSelect,
}: {
  active: SettingsSection
  onSelect: (section: SettingsSection) => void
}) {
  return (
    <div className="w-44 flex-none overflow-y-auto border-r border-border bg-panel py-2">
      {ITEMS.map((item) => (
        <button
          key={item.key}
          onClick={() => onSelect(item.key)}
          className={cn(
            'block w-full px-3.5 py-1.5 text-left text-xs font-medium text-sub hover:bg-panel2 hover:text-foreground',
            active === item.key && 'bg-panel2 text-foreground'
          )}
        >
          {item.label}
        </button>
      ))}
    </div>
  )
}
