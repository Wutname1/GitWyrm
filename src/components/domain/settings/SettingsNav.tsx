import { useMemo, useRef, useState } from 'react'
import { ArrowLeft, Search, X } from 'lucide-react'
import { cn } from '@/lib/utils'
import { SECTION_LABELS, searchSettings } from '@/lib/settingsSearch'
import type { SettingsSection } from '@/stores/uiStore'
import { useUiStore } from '@/stores/uiStore'

type NavItem = { key: SettingsSection; label: string }

const APP_ITEMS: NavItem[] = [
  { key: 'general', label: 'General' },
  { key: 'behavior', label: 'Behavior' },
  { key: 'tags', label: 'Tags' },
  { key: 'profiles', label: 'Profiles' },
  { key: 'ai', label: 'AI' },
  { key: 'openspec', label: 'OpenSpec' },
  { key: 'security', label: 'Security' },
  { key: 'appearance', label: 'Appearance' },
  { key: 'logs', label: 'Logs' },
  { key: 'about', label: 'About' },
]

const REPO_ITEMS: NavItem[] = [
  { key: 'repository', label: 'Repository' },
  { key: 'repositoryTags', label: 'Tags' },
]

export function SettingsNav({
  active,
  onSelect,
}: {
  active: SettingsSection
  onSelect: (section: SettingsSection) => void
}) {
  const showGraph = useUiStore((s) => s.showGraph)
  const revealSettingById = useUiStore((s) => s.revealSettingById)
  const [query, setQuery] = useState('')
  const inputRef = useRef<HTMLInputElement>(null)

  const results = useMemo(() => searchSettings(query), [query])
  const searching = query.trim().length > 0

  const clear = () => {
    setQuery('')
    inputRef.current?.focus()
  }

  const pick = (section: SettingsSection, id: string) => {
    revealSettingById(section, id)
    setQuery('')
  }

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

      <div className="flex-none border-b border-border p-2">
        <div className="relative">
          <Search
            size={13}
            className="pointer-events-none absolute left-2.5 top-1/2 -translate-y-1/2 text-muted-foreground"
          />
          <input
            ref={inputRef}
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === 'Escape' && query) {
                e.stopPropagation()
                clear()
              }
              if (e.key === 'Enter' && results[0]) {
                pick(results[0].section, results[0].id)
              }
            }}
            placeholder="Search settings"
            aria-label="Search settings"
            className="h-7 w-full rounded-md border border-input bg-background pl-7 pr-7 text-xs text-foreground outline-none placeholder:text-muted-foreground focus:border-ring"
          />
          {query && (
            <button
              onClick={clear}
              aria-label="Clear search"
              className="absolute right-1.5 top-1/2 grid size-4 -translate-y-1/2 place-items-center rounded text-muted-foreground hover:text-foreground"
            >
              <X size={12} />
            </button>
          )}
        </div>
      </div>

      <div className="min-h-0 flex-1 overflow-y-auto p-2">
        {searching ? (
          <SearchResults results={results} onPick={pick} />
        ) : (
          <>
            <div className="px-3 pb-1 pt-2 text-[10px] font-bold uppercase tracking-[.08em] text-muted-foreground">
              Application
            </div>
            {APP_ITEMS.map(renderItem)}

            <div className="mx-3 my-2 border-t border-border" />

            <div className="px-3 pb-1 pt-1 text-[10px] font-bold uppercase tracking-[.08em] text-muted-foreground">
              This repository
            </div>
            {REPO_ITEMS.map(renderItem)}
          </>
        )}
      </div>
    </div>
  )
}

function SearchResults({
  results,
  onPick,
}: {
  results: ReturnType<typeof searchSettings>
  onPick: (section: SettingsSection, id: string) => void
}) {
  if (results.length === 0) {
    return (
      <div className="px-3 py-4 text-2xs leading-relaxed text-muted-foreground">
        Nothing matches that. Try a simpler word, like <span className="text-foreground">theme</span>{' '}
        or <span className="text-foreground">commit</span>.
      </div>
    )
  }

  return (
    <div className="grid gap-0.5">
      <div className="px-3 pb-1 pt-1 text-[10px] font-bold uppercase tracking-[.08em] text-muted-foreground">
        {results.length} {results.length === 1 ? 'result' : 'results'}
      </div>
      {results.map((r) => (
        <button
          key={`${r.section}:${r.id}`}
          onClick={() => onPick(r.section, r.id)}
          aria-label={`${r.label}, in ${SECTION_LABELS[r.section]}`}
          className="block w-full rounded-md px-3 py-1.5 text-left hover:bg-panel2"
        >
          <div className="truncate text-xs font-medium text-foreground">{r.label}</div>
          <div className="truncate text-[10px] text-muted-foreground">
            {SECTION_LABELS[r.section]}
          </div>
        </button>
      ))}
    </div>
  )
}
