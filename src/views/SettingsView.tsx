import { lazy, Suspense, useEffect, useState } from 'react'
import { SettingsNav } from '@/components/domain/settings/SettingsNav'
import { Dialog, DialogContent, DialogTitle } from '@/components/ui/dialog'
import {
  Bot,
  CircleHelp,
  FileText,
  FolderGit2,
  Info,
  NotebookTabs,
  Palette,
  Plug,
  Settings2,
  ShieldCheck,
  Tags,
  Users,
  X,
  Zap,
  type LucideIcon,
} from 'lucide-react'
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
  integrations: 'Integrations',
  openspec: 'OpenSpec',
  security: 'Security',
  appearance: 'Appearance',
  logs: 'Logs',
  about: 'About',
}

const SUBTITLES: Partial<Record<SettingsSection, string>> = {
  general: 'The basics for opening projects, switching branches, and making commits.',
  behavior: 'How GitWyrm acts while you work.',
  repository: 'These settings apply only to the repository open in the active tab.',
  repositoryTags: 'Tag rules for the repository open in the active tab.',
  tags: 'The default tag rules for every repository.',
  profiles: 'Who you commit as, and the key you sign with.',
  ai: 'Choose the model that helps write commit messages and summaries.',
  integrations: 'Connect GitWyrm to the site your code is hosted on.',
  openspec: 'How GitWyrm works with your openspec/ folder.',
  security: 'Prove your commits came from you, and choose the programs GitWyrm uses.',
  appearance: 'Make GitWyrm comfortable to read and easy to recognize.',
  logs: 'Get useful details when something does not work as expected.',
  about: 'Version, updates, and local app settings.',
}

const ICONS: Record<SettingsSection, LucideIcon> = {
  general: Settings2,
  behavior: Zap,
  repository: FolderGit2,
  repositoryTags: Tags,
  tags: Tags,
  profiles: Users,
  ai: Bot,
  integrations: Plug,
  openspec: NotebookTabs,
  security: ShieldCheck,
  appearance: Palette,
  logs: FileText,
  about: Info,
}

/**
 * Section bodies are split out and loaded on demand.
 *
 * Statically importing all twelve put every one of them in the main bundle and
 * evaluated them at startup, to show the single section the user asked for.
 * Several are large -- Security is ~670 lines and fires `getEffectiveIdentity`
 * on mount, Profiles ~550, AI ~490 -- and the whole cost landed in the frame
 * the dialog's open animation starts in, which is what made the fade stutter.
 */
const SECTION_BODIES: Record<SettingsSection, React.ComponentType> = {
  general: lazy(async () => ({
    default: (await import('@/components/domain/settings/GeneralSettings')).GeneralSettings,
  })),
  behavior: lazy(async () => ({
    default: (await import('@/components/domain/settings/BehaviorSettings')).BehaviorSettings,
  })),
  repository: lazy(async () => ({
    default: (await import('@/components/domain/settings/RepositorySettings')).RepositorySettings,
  })),
  repositoryTags: lazy(async () => ({
    default: (await import('@/components/domain/settings/RepositoryTagsSettings'))
      .RepositoryTagsSettings,
  })),
  tags: lazy(async () => ({
    default: (await import('@/components/domain/settings/TagsSettings')).TagsSettings,
  })),
  profiles: lazy(async () => ({
    default: (await import('@/components/domain/settings/ProfilesSettings')).ProfilesSettings,
  })),
  ai: lazy(async () => ({
    default: (await import('@/components/domain/settings/AiSettings')).AiSettings,
  })),
  integrations: lazy(async () => ({
    default: (await import('@/components/domain/settings/IntegrationsSettings'))
      .IntegrationsSettings,
  })),
  openspec: lazy(async () => ({
    default: (await import('@/components/domain/settings/OpenspecSettings')).OpenspecSettings,
  })),
  security: lazy(async () => ({
    default: (await import('@/components/domain/settings/SecuritySettings')).SecuritySettings,
  })),
  appearance: lazy(async () => ({
    default: (await import('@/components/domain/settings/AppearanceSettings')).AppearanceSettings,
  })),
  logs: lazy(async () => ({
    default: (await import('@/components/domain/settings/LogsSettings')).LogsSettings,
  })),
  about: lazy(async () => ({
    default: (await import('@/components/domain/settings/AboutSettings')).AboutSettings,
  })),
}

/** How long the dialog's open animation runs; see `DialogContent`'s duration-200. */
const OPEN_ANIMATION_MS = 200

/**
 * Whether the dialog has finished animating open.
 *
 * Mounting the body during the fade is what the user sees as a stutter: React
 * commits a page's worth of settings on the same thread that has to paint the
 * next animation frame. Waiting until the animation is done trades a moment of
 * empty panel for a fade that actually runs, and the panel is not blank -- the
 * chrome, nav rail and heading are all up immediately.
 *
 * Resets on close so the next open animates from a clean state rather than
 * snapping straight to full content.
 */
function useSettledOpen(open: boolean): boolean {
  const [settled, setSettled] = useState(false)

  useEffect(() => {
    if (!open) {
      setSettled(false)
      return
    }
    const timer = setTimeout(() => setSettled(true), OPEN_ANIMATION_MS)
    return () => clearTimeout(timer)
  }, [open])

  return settled
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

  const settled = useSettledOpen(open)
  const SectionBody = SECTION_BODIES[settingsSection]
  const subtitle = SUBTITLES[settingsSection]
  const SectionIcon = ICONS[settingsSection] ?? CircleHelp

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
                <div className="flex items-start gap-3 border-b border-border pb-5">
                  <div className="grid size-9 flex-none place-items-center rounded-lg bg-[var(--gw-accent-soft)] text-accent-text">
                    <SectionIcon size={18} />
                  </div>
                  <div className="min-w-0">
                    <h2 className="text-lg font-semibold text-foreground">{TITLES[settingsSection]}</h2>
                    {subtitle && <p className="mt-0.5 text-xs text-muted-foreground">{subtitle}</p>}
                  </div>
                  <div className="ml-auto flex items-center gap-1.5 pt-1 text-2xs text-muted-foreground">
                    <span className="size-1.5 rounded-full bg-[var(--gw-accent)]" />
                    Saved automatically
                  </div>
                </div>
                {settled && (
                  <Suspense fallback={null}>
                    <div className="pb-4">
                      <SectionBody />
                    </div>
                  </Suspense>
                )}
              </div>
            </div>
          </div>
        </div>
      </DialogContent>
    </Dialog>
  )
}
