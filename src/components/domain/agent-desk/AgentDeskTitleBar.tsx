import { Plus } from 'lucide-react'
import { useWindowWidth } from '@/hooks/useWindowWidth'
import { isCompactWidth, isNarrowWidth } from '@/lib/agentDeskDockPlacement'
import { WindowControls } from '@/components/domain/WindowControls'
import { AiProviderChip } from '@/components/domain/spec-desk/AiProviderChip'
import { Button } from '@/components/ui/button'
import { cn } from '@/lib/utils'
import { titleBarInsetStyle } from '@/lib/platform'

type CenterView = 'conversation' | 'openspec' | 'setup' | 'import'

/**
 * Agent Desk's own titlebar. Window decorations are off app-wide, so this
 * draws the drag region and window buttons, matching `DeskTitleBar` and the
 * main window's chrome.
 */
/**
 * The window's sections, in the order they are shown.
 *
 * `short` is what the tab says once the window is too narrow to carry both
 * these four labels and the actions on the right -- at the 720px minimum the
 * full words ran straight into the New chat button. The full label stays as
 * the accessible name and the tooltip, so nothing is lost but the width.
 */
const SECTIONS: { id: CenterView; label: string; short: string }[] = [
  { id: 'conversation', label: 'Chats', short: 'Chats' },
  { id: 'openspec', label: 'OpenSpec', short: 'Spec' },
  { id: 'setup', label: 'Agent setup', short: 'Setup' },
  { id: 'import', label: 'Import chats', short: 'Import' },
]

export function AgentDeskTitleBar({
  repoName,
  repoId,
  onNewChat,
  centerView,
  onChangeCenterView,
}: {
  repoName: string
  repoId: string | null
  onNewChat: () => void
  centerView: CenterView
  onChangeCenterView: (view: CenterView) => void
}) {
  const windowWidth = useWindowWidth()
  const compact = isCompactWidth(windowWidth)
  const narrow = isNarrowWidth(windowWidth)
  return (
    <div
      data-tauri-drag-region
      style={titleBarInsetStyle()}
      className="flex h-10 flex-none select-none items-center gap-2.5 border-b border-border bg-panel pl-3 pr-0"
    >
      <span className="text-xs font-semibold text-foreground">Agent Desk</span>
      {/* The window's own title already names the repository, so this is the
          first thing to drop when the bar runs out of room. */}
      {!compact && (
        <span className="min-w-0 overflow-hidden text-ellipsis whitespace-nowrap text-2xs text-muted-foreground">
          {repoName}
        </span>
      )}

      {/* Task 2.3: OpenSpec's change list/tasks/handoff actions stay one
          click away instead of disappearing into the new session shell.

          Sized to be seen. These were 10px text with a hairline underline,
          which read as decoration next to the window controls and left people
          not realising the window had sections at all. The active one now
          carries a filled background rather than only a rule, because an
          underline that thin is the first thing lost against a dark panel. */}
      <nav
        aria-label="Agent Desk sections"
        className={cn('flex min-w-0 items-center gap-1', compact ? 'ml-1' : 'ml-5')}
      >
        {SECTIONS.map((section) => (
          <button
            key={section.id}
            type="button"
            onClick={() => onChangeCenterView(section.id)}
            disabled={!repoId}
            aria-current={centerView === section.id ? 'page' : undefined}
            // The visible word shortens; the accessible name never does.
            aria-label={section.label}
            title={section.label}
            className={cn(
              'flex-none rounded-md py-1 text-xs font-medium transition-colors disabled:opacity-40',
              compact ? 'px-1.5' : 'px-2.5',
              centerView === section.id
                ? 'bg-soft text-accent-text'
                : 'text-sub hover:bg-panel3 hover:text-foreground'
            )}
          >
            {compact ? section.short : section.label}
          </button>
        ))}
      </nav>

      <div className={cn('ml-auto flex h-full flex-none items-center gap-2', compact ? 'pl-1' : 'pl-3')}>
        <Button
          size="sm"
          variant="secondary"
          onClick={onNewChat}
          disabled={!repoId}
          // The design system's own tooltip, which also supplies the
          // accessible name -- so the icon-only form still announces itself.
          tooltip={narrow ? 'New chat' : undefined}
          className={narrow ? 'px-2' : undefined}
        >
          {narrow ? <Plus size={14} aria-hidden /> : 'New chat'}
        </Button>
        {/* Which AI is selected is also shown on the composer, right where a
            message is about to be sent, so this copy of it is what goes when
            the bar is tight rather than something with no other home. */}
        {!compact && <AiProviderChip repoId={repoId} scope="writing" />}
        <div className="flex h-full items-stretch">
          <WindowControls />
        </div>
      </div>
    </div>
  )
}
