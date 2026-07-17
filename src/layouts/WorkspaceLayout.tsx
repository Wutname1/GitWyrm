import { TabBar } from '@/components/domain/TabBar'
import { Toolbar } from '@/components/domain/Toolbar'
import { LeftPanel } from '@/components/domain/left-panel/LeftPanel'
import { RightPanel } from '@/components/domain/RightPanel'
import { StatusBar } from '@/components/domain/StatusBar'
import { GraphView } from '@/views/GraphView'
import { DiffView } from '@/views/DiffView'
import { SettingsView } from '@/views/SettingsView'
import { useUiStore } from '@/stores/uiStore'

function CenterView() {
  const view = useUiStore((s) => s.centerView)
  if (view === 'diff') return <DiffView />
  if (view === 'settings') return <SettingsView />
  return <GraphView />
}

export function WorkspaceLayout() {
  return (
    <div className="flex h-dvh w-full flex-col overflow-hidden bg-background text-foreground">
      <TabBar />
      <Toolbar />
      <div className="flex min-h-0 flex-1">
        <LeftPanel />
        <div className="flex min-w-0 flex-1 flex-col">
          <CenterView />
        </div>
        <RightPanel />
      </div>
      <StatusBar />
    </div>
  )
}
