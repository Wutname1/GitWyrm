import { useEffect, useState } from 'react'
import { TabBar, VerticalTabRail } from '@/components/domain/TabBar'
import { Toolbar } from '@/components/domain/Toolbar'
import { MergeBanner } from '@/components/domain/MergeBanner'
import { LeftPanel } from '@/components/domain/left-panel/LeftPanel'
import { RightPanel } from '@/components/domain/RightPanel'
import { StatusBar } from '@/components/domain/StatusBar'
import { GraphView, WIP_SHA } from '@/views/GraphView'
import { DiffView } from '@/views/DiffView'
import { SettingsView } from '@/views/SettingsView'
import { RepoPickerView } from '@/components/modals/RepoPickerModal'
import { ConflictView } from '@/views/ConflictView'
import { GithubView } from '@/views/GithubView'
import { FileHistoryView } from '@/views/FileHistoryView'
import { BlameView } from '@/views/BlameView'
import { CommitDrawer } from '@/components/domain/graph/CommitDrawer'
import { MultiCommitDrawer } from '@/components/domain/graph/MultiCommitDrawer'
import { useUiStore } from '@/stores/uiStore'
import {
  useActiveRepo,
  DEFAULT_DRAWER_HEIGHT,
  DEFAULT_LEFT_PANEL_WIDTH,
  DEFAULT_RIGHT_PANEL_WIDTH,
  MAX_DRAWER_HEIGHT,
  MIN_DRAWER_HEIGHT,
  MAX_LEFT_PANEL_WIDTH,
  MAX_RIGHT_PANEL_WIDTH,
  MIN_LEFT_PANEL_WIDTH,
  MIN_RIGHT_PANEL_WIDTH,
  useWorkspaceStore,
} from '@/stores/workspaceStore'
import { ResizeHandle } from '@/components/ui/ResizeHandle'
import { cn } from '@/lib/utils'
import { RepositoryPreviewCapture } from '@/components/domain/RepositoryPreviewCapture'

function CenterView() {
  const view = useUiStore((s) => s.centerView)
  if (view === 'diff') return <DiffView />
  if (view === 'conflict') return <ConflictView />
  if (view === 'github') return <GithubView />
  if (view === 'fileHistory') return <FileHistoryView />
  if (view === 'blame') return <BlameView />
  return <GraphView />
}

/**
 * The selected commit's files, pinned under the center view. It lives here
 * rather than inside GraphView so that opening one of its files -- which swaps
 * the view above to the diff -- leaves the file list in place to click through.
 */
function CommitDrawerSlot() {
  const repo = useActiveRepo()
  const selectedSha = useUiStore((s) => s.selectedSha)
  const selectedShas = useUiStore((s) => s.selectedShas)
  const drawerHeight = useWorkspaceStore((s) => s.drawerHeight)
  const setDrawerHeight = useWorkspaceStore((s) => s.setDrawerHeight)
  const multiCount = selectedShas.length > 1 ? selectedShas.length : 0
  // Shift/Ctrl-clicking builds the selection up in the graph, far from this
  // panel, so the combined view can change without anything drawing the eye to
  // it. Flash the edge whenever the group's size changes -- including each
  // extra commit added, not just the jump from one to many.
  const [flash, setFlash] = useState(false)
  useEffect(() => {
    if (multiCount === 0) return
    setFlash(true)
    const timer = window.setTimeout(() => setFlash(false), 1200)
    return () => window.clearTimeout(timer)
  }, [multiCount])

  if (!repo) return null
  if (selectedSha == null || selectedSha === WIP_SHA) return null
  return (
    <div
      className={cn(
        'relative flex-none transition-[box-shadow,background-color] duration-500',
        flash && 'bg-primary/5 shadow-[inset_0_2px_0_var(--gw-accent),inset_0_-1px_0_var(--gw-accent)]'
      )}
      style={{ height: drawerHeight }}
    >
      {/* Handle on the drawer's top edge: dragging up makes it taller. */}
      <ResizeHandle
        ariaLabel="Resize commit details"
        axis="y"
        direction={-1}
        value={drawerHeight}
        min={MIN_DRAWER_HEIGHT}
        max={MAX_DRAWER_HEIGHT}
        defaultValue={DEFAULT_DRAWER_HEIGHT}
        onChange={setDrawerHeight}
        className="-top-1"
      />
      {selectedShas.length > 1 ? (
        // Two or more selected commits: show every change they made, combined.
        <MultiCommitDrawer repoId={repo.id} shas={selectedShas} />
      ) : (
        <CommitDrawer repoId={repo.id} sha={selectedSha} />
      )}
    </div>
  )
}

export function WorkspaceLayout() {
  const tabLayout = useWorkspaceStore((state) => state.tabLayout)
  const leftPanelWidth = useWorkspaceStore((state) => state.leftPanelWidth)
  const rightPanelWidth = useWorkspaceStore((state) => state.rightPanelWidth)
  const setLeftPanelWidth = useWorkspaceStore((state) => state.setLeftPanelWidth)
  const setRightPanelWidth = useWorkspaceStore((state) => state.setRightPanelWidth)
  const centerView = useUiStore((s) => s.centerView)
  const inRepoPicker = centerView === 'repoPicker'

  const panelRow = (
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
        <CommitDrawerSlot />
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
  )

  // Settings is not in this list: it opens as a modal over whatever is here.
  const centerBody = inRepoPicker ? <RepoPickerView /> : panelRow

  const workspaceBody = (
    <div
      data-repository-preview-root
      className="flex min-h-0 flex-1 flex-col bg-background"
    >
      {!inRepoPicker && <Toolbar />}
      {!inRepoPicker && <MergeBanner />}
      {centerBody}
      <StatusBar />
    </div>
  )

  return (
    <div className="flex h-full w-full flex-col overflow-hidden bg-background text-foreground">
      <RepositoryPreviewCapture />
      <SettingsView />
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
