import { AboutSettings } from '@/components/domain/settings/AboutSettings'
import { AiSettings } from '@/components/domain/settings/AiSettings'
import { AppearanceSettings } from '@/components/domain/settings/AppearanceSettings'
import { GeneralSettings } from '@/components/domain/settings/GeneralSettings'
import { LogsSettings } from '@/components/domain/settings/LogsSettings'
import { RepositorySettings } from '@/components/domain/settings/RepositorySettings'
import { RepositoryTagsSettings } from '@/components/domain/settings/RepositoryTagsSettings'
import { SettingsNav } from '@/components/domain/settings/SettingsNav'
import { TagsSettings } from '@/components/domain/settings/TagsSettings'
import { Separator } from '@/components/ui/separator'
import type { SettingsSection } from '@/stores/uiStore'
import { useUiStore } from '@/stores/uiStore'

const TITLES: Record<SettingsSection, string> = {
  general: 'General',
  repository: 'Repository',
  repositoryTags: 'Tags',
  tags: 'Tags',
  ai: 'AI',
  appearance: 'Appearance',
  logs: 'Logs',
  about: 'About',
}

const SUBTITLES: Partial<Record<SettingsSection, string>> = {
  repository: 'These settings apply only to the repository open in the active tab.',
  repositoryTags: 'Tag rules for the repository open in the active tab.',
  tags: 'The default tag rules for every repository.',
}

const SECTION_BODIES: Record<SettingsSection, React.ComponentType> = {
  general: GeneralSettings,
  repository: RepositorySettings,
  repositoryTags: RepositoryTagsSettings,
  tags: TagsSettings,
  ai: AiSettings,
  appearance: AppearanceSettings,
  logs: LogsSettings,
  about: AboutSettings,
}

export function SettingsView() {
  const settingsSection = useUiStore((s) => s.settingsSection)
  const setSettingsSection = useUiStore((s) => s.setSettingsSection)

  const SectionBody = SECTION_BODIES[settingsSection]
  const subtitle = SUBTITLES[settingsSection]

  return (
    <div className="flex min-h-0 flex-1 bg-background">
      <SettingsNav active={settingsSection} onSelect={setSettingsSection} />
      <div className="flex min-h-0 flex-1 flex-col">
        <div className="flex h-10 flex-none items-center gap-2.5 border-b border-border bg-panel px-4">
          <span className="text-xs font-bold tracking-[.05em] text-sub">SETTINGS</span>
        </div>
        <div className="min-h-0 flex-1 overflow-y-auto px-8 py-6">
          <div className="mx-auto max-w-4xl">
            <h2 className="text-base font-bold text-foreground">{TITLES[settingsSection]}</h2>
            {subtitle && <p className="mt-0.5 text-2xs text-muted-foreground">{subtitle}</p>}
            <Separator className="mt-3" />
            <SectionBody />
          </div>
        </div>
      </div>
    </div>
  )
}
