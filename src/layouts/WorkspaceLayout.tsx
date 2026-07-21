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
import { useWorkspaceStore } from '@/stores/workspaceStore'

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

  const workspaceBody = (
    <>
      <Toolbar />
      <MergeBanner />
      <div className="flex min-h-0 flex-1">
        <LeftPanel />
        <div className="flex min-w-0 flex-1 flex-col">
          <CenterView />
        </div>
        <RightPanel />
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
