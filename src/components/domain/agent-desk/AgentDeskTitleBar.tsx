import { WindowControls } from '@/components/domain/WindowControls'
import { AiProviderChip } from '@/components/domain/spec-desk/AiProviderChip'
import { Button } from '@/components/ui/button'
import { cn } from '@/lib/utils'

type CenterView = 'conversation' | 'openspec' | 'setup'

/**
 * Agent Desk's own titlebar. Window decorations are off app-wide, so this
 * draws the drag region and window buttons, matching `DeskTitleBar` and the
 * main window's chrome.
 */
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
          click away instead of disappearing into the new session shell. */}
      <div className="ml-4 flex h-full items-stretch gap-0.5 self-stretch">
        <button
          type="button"
          onClick={() => onChangeCenterView('conversation')}
          disabled={!repoId}
          className={cn(
            'border-b-2 px-1 text-2xs font-semibold transition-colors',
            centerView === 'conversation'
              ? 'border-primary text-foreground'
              : 'border-transparent text-sub hover:text-foreground'
          )}
        >
          Chats
        </button>
        <button
          type="button"
          onClick={() => onChangeCenterView('openspec')}
          disabled={!repoId}
          className={cn(
            'border-b-2 px-1 text-2xs font-semibold transition-colors',
            centerView === 'openspec'
              ? 'border-primary text-foreground'
              : 'border-transparent text-sub hover:text-foreground'
          )}
        >
          OpenSpec
        </button>
        <button
          type="button"
          onClick={() => onChangeCenterView('setup')}
          className={cn(
            'border-b-2 px-1 text-2xs font-semibold transition-colors',
            centerView === 'setup'
              ? 'border-primary text-foreground'
              : 'border-transparent text-sub hover:text-foreground'
          )}
        >
          Agent setup
        </button>
      </div>

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
