import { X } from 'lucide-react'
import { AboutSettings } from '@/components/domain/settings/AboutSettings'
import { AiSettings } from '@/components/domain/settings/AiSettings'
import { AppearanceSettings } from '@/components/domain/settings/AppearanceSettings'
import { GeneralSettings } from '@/components/domain/settings/GeneralSettings'
import { LogsSettings } from '@/components/domain/settings/LogsSettings'
import { SettingsNav } from '@/components/domain/settings/SettingsNav'
import { Separator } from '@/components/ui/separator'
import { TooltipButton } from '@/components/ui/tooltip'
import type { SettingsSection } from '@/stores/uiStore'
import { useUiStore } from '@/stores/uiStore'

const TITLES: Record<SettingsSection, string> = {
  general: 'General',
  ai: 'AI',
  appearance: 'Appearance',
  logs: 'Logs',
  about: 'About',
}

const SECTION_BODIES: Record<SettingsSection, React.ComponentType> = {
  general: GeneralSettings,
  ai: AiSettings,
  appearance: AppearanceSettings,
  logs: LogsSettings,
  about: AboutSettings,
}

export function SettingsView() {
  const showGraph = useUiStore((s) => s.showGraph)
  const settingsSection = useUiStore((s) => s.settingsSection)
  const setSettingsSection = useUiStore((s) => s.setSettingsSection)

  const SectionBody = SECTION_BODIES[settingsSection]

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div className="flex h-10 flex-none items-center gap-2.5 border-b border-border bg-panel px-3.5">
        <span className="text-xs font-bold tracking-[.05em] text-sub">SETTINGS</span>
        <div className="flex-1" />
        <TooltipButton
          onClick={showGraph}
          tooltip="Back to graph"
          className="flex size-6 flex-none items-center justify-center rounded-[5px] border border-border bg-panel2 text-xs text-sub hover:border-muted-foreground hover:bg-panel3"
        >
          <X size={12} />
        </TooltipButton>
      </div>
      <div className="flex min-h-0 flex-1">
        <SettingsNav active={settingsSection} onSelect={setSettingsSection} />
        <div className="min-h-0 flex-1 overflow-y-auto px-6 py-4">
          <div className="max-w-2xl">
            <h2 className="mb-1 text-sm font-bold text-foreground">{TITLES[settingsSection]}</h2>
            <Separator />
            <SectionBody />
          </div>
        </div>
      </div>
    </div>
  )
}
