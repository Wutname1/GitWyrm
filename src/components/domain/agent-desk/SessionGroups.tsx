import { useMemo, useState } from 'react'
import { useAgentDeskUiStore } from '@/stores/agentDeskUiStore'
import { Clock3, FileDiff, Folders } from 'lucide-react'
import type { AgentSessionHeader } from '@/lib/bindings'
import { buildSidebarRows, sidebarEmptyMessage, type SidebarGroupMode } from '@/lib/agentSessionGrouping'
import { VirtualSessionList } from '@/components/domain/agent-desk/VirtualSessionList'
import { cn } from '@/lib/utils'

const MODES: { id: SidebarGroupMode; label: string; icon: typeof Clock3 }[] = [
  { id: 'recent', label: 'Recent', icon: Clock3 },
  { id: 'project', label: 'Project', icon: Folders },
  { id: 'diff', label: 'Diff', icon: FileDiff },
]

/**
 * Grouping-mode picker (mockup `.ag-sidebar-filters`) plus the collapsible
 * group headers tasks.md 3.3 requires -- the mockup's headers are compact but
 * have no collapse affordance at all, so collapse state is new here, kept per
 * grouping mode (a group collapsed under Project should not also affect
 * Recent) and keyed by the group's row ID, which `buildSidebarRows` derives
 * deterministically from the grouping key (day bucket / normalized repo path
 * / diff bucket label) so it survives the header list re-rendering.
 */
export function SessionGroups({
  headers,
  selectedId,
  onSelectSession,
  onRename,
  onArchive,
  onDelete,
  searchTerm,
  archived,
  failed = false,
}: {
  headers: AgentSessionHeader[]
  /** Showing the archived list, so an empty one must not read as "no chats". */
  archived?: boolean
  /** What the sidebar is filtering by, so an empty list can say WHY it is empty. */
  searchTerm?: string
  /** True when the chat list could not be read, so an empty list is not an answer. */
  failed?: boolean
  selectedId: string | null
  onSelectSession: (sessionId: string) => void
  onRename: (sessionId: string, title: string) => void
  onArchive: (sessionId: string, archived: boolean) => void
  onDelete: (sessionId: string, title: string) => void
}) {
  // Persisted, not component-local. `architecture.md` lists "sidebar
  // grouping" among what `agentDeskUiStore` holds, and it did not -- so a
  // person who works by project re-picked it on every launch.
  const mode = useAgentDeskUiStore((s) => s.layout.sidebarGrouping)
  const setMode = useAgentDeskUiStore((s) => s.setSidebarGrouping)
  // Separate collapse sets per mode: collapsing "Today" in Recent should not
  // leave a same-named/keyed group collapsed if the user switches to Project.
  const [collapsedByMode, setCollapsedByMode] = useState<Record<SidebarGroupMode, Set<string>>>({
    recent: new Set(),
    project: new Set(),
    diff: new Set(),
  })

  const rows = useMemo(
    () => buildSidebarRows(headers, { mode, collapsedGroupIds: collapsedByMode[mode] }),
    [headers, mode, collapsedByMode]
  )

  const toggleGroup = (groupId: string) => {
    setCollapsedByMode((prev) => {
      const next = new Set(prev[mode])
      if (next.has(groupId)) next.delete(groupId)
      else next.add(groupId)
      return { ...prev, [mode]: next }
    })
  }

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div className="flex gap-0.5 px-1.5 pt-1.5" role="tablist" aria-label="Arrange chats">
        {MODES.map(({ id, label, icon: Icon }) => (
          <button
            key={id}
            type="button"
            role="tab"
            aria-selected={mode === id}
            onClick={() => setMode(id)}
            className={cn(
              // A one-step tonal shift is not a selected state on its own
              // (DESIGN.md); the mint edge is what makes it read.
              'flex h-6 flex-1 items-center justify-center gap-1 rounded border-b-2 text-2xs',
              mode === id
                ? 'border-primary bg-panel2 text-foreground'
                : 'border-transparent text-muted-foreground hover:bg-panel2 hover:text-foreground'
            )}
          >
            <Icon size={11} />
            {label}
          </button>
        ))}
      </div>

      <VirtualSessionList
        rows={rows}
        selectedId={selectedId}
        onSelectSession={onSelectSession}
        onRename={onRename}
        onArchive={onArchive}
        onDelete={onDelete}
        onToggleGroup={toggleGroup}
        emptyMessage={sidebarEmptyMessage({
          failed,
          searchTerm,
          archived,
          headerCount: headers.length,
        })}
      />
    </div>
  )
}
