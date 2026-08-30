import { WindowControls } from '@/components/domain/WindowControls'
import { AiProviderChip } from '@/components/domain/spec-desk/AiProviderChip'
import { Button } from '@/components/ui/button'
import { cn } from '@/lib/utils'

type CenterView = 'conversation' | 'openspec' | 'setup' | 'import'

/**
 * Agent Desk's own titlebar. Window decorations are off app-wide, so this
 * draws the drag region and window buttons, matching `DeskTitleBar` and the
 * main window's chrome.
 */
/** The window's sections, in the order they are shown. */
const SECTIONS: { id: CenterView; label: string }[] = [
  { id: 'conversation', label: 'Chats' },
  { id: 'openspec', label: 'OpenSpec' },
  { id: 'setup', label: 'Agent setup' },
  { id: 'import', label: 'Import chats' },
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
  return (
    <div
      data-tauri-drag-region
      className="flex h-10 flex-none select-none items-center gap-2.5 border-b border-border bg-panel pl-3 pr-0"
    >
      <span className="text-xs font-semibold text-foreground">Agent Desk</span>
      <span className="overflow-hidden text-ellipsis whitespace-nowrap text-2xs text-muted-foreground">
        {repoName}
      </span>

      {/* Task 2.3: OpenSpec's change list/tasks/handoff actions stay one
          click away instead of disappearing into the new session shell.

          Sized to be seen. These were 10px text with a hairline underline,
          which read as decoration next to the window controls and left people
          not realising the window had sections at all. The active one now
          carries a filled background rather than only a rule, because an
          underline that thin is the first thing lost against a dark panel. */}
      <nav aria-label="Agent Desk sections" className="ml-5 flex items-center gap-1">
        {SECTIONS.map((section) => (
          <button
            key={section.id}
            type="button"
            onClick={() => onChangeCenterView(section.id)}
            disabled={!repoId}
            aria-current={centerView === section.id ? 'page' : undefined}
            className={cn(
              'rounded-md px-2.5 py-1 text-xs font-medium transition-colors disabled:opacity-40',
              centerView === section.id
                ? 'bg-soft text-accent-text'
                : 'text-sub hover:bg-panel3 hover:text-foreground'
            )}
          >
            {section.label}
          </button>
        ))}
      </nav>

      <div className="ml-auto flex h-full items-center gap-2 pl-3">
        <Button size="sm" variant="secondary" onClick={onNewChat} disabled={!repoId}>
          New chat
        </Button>
        <AiProviderChip repoId={repoId} />
        <div className="flex h-full items-stretch">
          <WindowControls />
        </div>
      </div>
    </div>
  )
}
