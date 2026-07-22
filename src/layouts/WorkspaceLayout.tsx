import { TabBar, VerticalTabRail } from '@/components/domain/TabBar'
import { Toolbar } from '@/components/domain/Toolbar'
import { MergeBanner } from '@/components/domain/MergeBanner'
import { LeftPanel } from '@/components/domain/left-panel/LeftPanel'
import { RightPanel } from '@/components/domain/RightPanel'
import { StatusBar } from '@/components/domain/StatusBar'
import { GraphView } from '@/views/GraphView'
import { DiffView } from '@/views/DiffView'
import { SettingsView } from '@/views/SettingsView'
import { ConflictView } from '@/views/ConflictView'
import { GithubView } from '@/views/GithubView'
import { useUiStore } from '@/stores/uiStore'
import {
  DEFAULT_LEFT_PANEL_WIDTH,
  DEFAULT_RIGHT_PANEL_WIDTH,
  MAX_LEFT_PANEL_WIDTH,
  MAX_RIGHT_PANEL_WIDTH,
  MIN_LEFT_PANEL_WIDTH,
  MIN_RIGHT_PANEL_WIDTH,
  useWorkspaceStore,
} from '@/stores/workspaceStore'
import { ResizeHandle } from '@/components/ui/ResizeHandle'

function CenterView() {
  const view = useUiStore((s) => s.centerView)
  if (view === 'diff') return <DiffView />
  if (view === 'settings') return <SettingsView />
  if (view === 'conflict') return <ConflictView />
  if (view === 'github') return <GithubView />
  return <GraphView />
}

export function WorkspaceLayout() {
  const tabLayout = useWorkspaceStore((state) => state.tabLayout)
  const leftPanelWidth = useWorkspaceStore((state) => state.leftPanelWidth)
  const rightPanelWidth = useWorkspaceStore((state) => state.rightPanelWidth)
  const setLeftPanelWidth = useWorkspaceStore((state) => state.setLeftPanelWidth)
  const setRightPanelWidth = useWorkspaceStore((state) => state.setRightPanelWidth)

  const workspaceBody = (
    <>
      <Toolbar />
      <MergeBanner />
      <div className="flex min-h-0 flex-1">
        <div className="relative flex min-h-0 flex-none" style={{ width: leftPanelWidth }}>
          <LeftPanel />
          <ResizeHandle
            ariaLabel="Resize branches and tags"
            value={leftPanelWidth}
            min={MIN_LEFT_PANEL_WIDTH}
            max={MAX_LEFT_PANEL_WIDTH}
            defaultValue={DEFAULT_LEFT_PANEL_WIDTH}
            onChange={setLeftPanelWidth}
            className="-right-1"
          />
        </div>
        <div className="flex min-w-0 flex-1 flex-col">
          <CenterView />
        </div>
        <div className="relative flex min-h-0 flex-none" style={{ width: rightPanelWidth }}>
          <ResizeHandle
            ariaLabel="Resize changes and commit panel"
            value={rightPanelWidth}
            min={MIN_RIGHT_PANEL_WIDTH}
            max={MAX_RIGHT_PANEL_WIDTH}
            defaultValue={DEFAULT_RIGHT_PANEL_WIDTH}
            direction={-1}
            onChange={setRightPanelWidth}
            className="-left-1"
          />
          <RightPanel />
        </div>
      </div>
      <StatusBar />
    </>
  )

  return (
    <div className="flex h-full w-full flex-col overflow-hidden bg-background text-foreground">
      <TabBar />
      {tabLayout === 'vertical' ? (
        <div className="flex min-h-0 flex-1">
          <VerticalTabRail />
          <div className="flex min-w-0 flex-1 flex-col">
            {workspaceBody}
          </div>
        </div>
      ) : workspaceBody}
    </div>
  )
}
