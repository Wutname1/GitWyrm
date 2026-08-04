import { AboutSettings } from '@/components/domain/settings/AboutSettings'
import { AiSettings } from '@/components/domain/settings/AiSettings'
import { AppearanceSettings } from '@/components/domain/settings/AppearanceSettings'
import { BehaviorSettings } from '@/components/domain/settings/BehaviorSettings'
import { GeneralSettings } from '@/components/domain/settings/GeneralSettings'
import { LogsSettings } from '@/components/domain/settings/LogsSettings'
import { OpenspecSettings } from '@/components/domain/settings/OpenspecSettings'
import { ProfilesSettings } from '@/components/domain/settings/ProfilesSettings'
import { RepositorySettings } from '@/components/domain/settings/RepositorySettings'
import { RepositoryTagsSettings } from '@/components/domain/settings/RepositoryTagsSettings'
import { SecuritySettings } from '@/components/domain/settings/SecuritySettings'
import { SettingsNav } from '@/components/domain/settings/SettingsNav'
import { TagsSettings } from '@/components/domain/settings/TagsSettings'
import { Separator } from '@/components/ui/separator'
import { Dialog, DialogContent, DialogTitle } from '@/components/ui/dialog'
import { X } from 'lucide-react'
import type { SettingsSection } from '@/stores/uiStore'
import { useUiStore } from '@/stores/uiStore'

const TITLES: Record<SettingsSection, string> = {
  general: 'General',
  behavior: 'Behavior',
  repository: 'Repository',
  repositoryTags: 'Tags',
  tags: 'Tags',
  profiles: 'Profiles',
  ai: 'AI',
  openspec: 'OpenSpec',
  security: 'Security',
  appearance: 'Appearance',
  logs: 'Logs',
  about: 'About',
}

const SUBTITLES: Partial<Record<SettingsSection, string>> = {
  behavior: 'How GitWyrm acts while you work.',
  repository: 'These settings apply only to the repository open in the active tab.',
  repositoryTags: 'Tag rules for the repository open in the active tab.',
  tags: 'The default tag rules for every repository.',
  profiles: 'Who you commit as, and the key you sign with.',
  openspec: 'How GitWyrm works with your openspec/ folder.',
  security: 'Prove your commits came from you, and choose the programs GitWyrm uses.',
}

const SECTION_BODIES: Record<SettingsSection, React.ComponentType> = {
  general: GeneralSettings,
  behavior: BehaviorSettings,
  repository: RepositorySettings,
  repositoryTags: RepositoryTagsSettings,
  tags: TagsSettings,
  profiles: ProfilesSettings,
  ai: AiSettings,
  openspec: OpenspecSettings,
  security: SecuritySettings,
  appearance: AppearanceSettings,
  logs: LogsSettings,
  about: AboutSettings,
}

/**
 * Settings, over the top of the workspace rather than in place of it.
 *
 * Settings is a detour: you come here to flip one switch and go back to what
 * you were doing. Swapping the whole center view for it threw away the graph,
 * the diff, and the selection every time. As a modal the work stays on screen
 * behind it, and the two ordinary ways out of a modal -- the X in the corner
 * and a click on the backdrop -- both land you back exactly where you were.
 */
export function SettingsView() {
  const open = useUiStore((s) => s.centerView === 'settings')
  const settingsSection = useUiStore((s) => s.settingsSection)
  const setSettingsSection = useUiStore((s) => s.setSettingsSection)
  const showGraph = useUiStore((s) => s.showGraph)

  const SectionBody = SECTION_BODIES[settingsSection]
  const subtitle = SUBTITLES[settingsSection]

  return (
    <Dialog open={open} onOpenChange={(next) => !next && showGraph()}>
      {/* Near-full-window: the nav rail plus a section body is a page's worth of
          content, and shrinking it to dialog width would reflow every settings
          row. `showCloseButton` is off because the default X sits over the nav
          rail; ours goes in the header bar on the right. */}
      <DialogContent
        showCloseButton={false}
        className="flex h-[88vh] w-[calc(100%-4rem)] max-w-6xl flex-col gap-0 overflow-hidden p-0 sm:max-w-6xl"
      >
        <div className="flex min-h-0 flex-1">
          <SettingsNav active={settingsSection} onSelect={setSettingsSection} />
          <div className="flex min-h-0 flex-1 flex-col">
            <div className="flex h-10 flex-none items-center gap-2.5 border-b border-border bg-panel px-4">
              <DialogTitle className="text-xs font-bold leading-none tracking-[.05em] text-sub">
                SETTINGS
              </DialogTitle>
              <button
                onClick={showGraph}
                aria-label="Close settings"
                className="ml-auto grid size-6 place-items-center rounded-md text-sub transition-colors hover:bg-panel2 hover:text-foreground"
              >
                <X size={15} />
              </button>
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
      </DialogContent>
    </Dialog>
  )
}
