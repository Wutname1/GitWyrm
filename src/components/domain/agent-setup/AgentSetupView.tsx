import { useState } from 'react'
import { AlertTriangle, RefreshCw } from 'lucide-react'
import type { InventoryEntry } from '@/lib/bindings'
import { useAgentConfigDetectedClients, useAgentConfigInventory } from '@/hooks/useAgentConfig'
import { hasAnyDifference } from '@/lib/agentConfig'
import { Button } from '@/components/ui/button'
import { cn } from '@/lib/utils'
import { SyncSummaryLine, SyncTable } from './SyncTable'
import { CopyPreviewDialog } from './CopyPreviewDialog'
import { BatchReviewDialog } from './BatchReviewDialog'
import { AgentCopiesOnDisk } from './AgentCopiesOnDisk'
import { RecentConfigChanges } from './RecentConfigChanges'
import { AgentCatalog } from './AgentCatalog'
import { DetectedAppsTab } from './DetectedAppsTab'

type SetupTab = 'skills' | 'connections' | 'providers' | 'detected' | 'disk'

const TABS: { id: SetupTab; label: string }[] = [
  { id: 'skills', label: 'Skills' },
  { id: 'connections', label: 'Connections' },
  { id: 'providers', label: 'AI tools' },
  { id: 'detected', label: 'Detected apps' },
  // Named for what the tab holds, not just its first section: it now also
  // lists the settings GitWyrm has changed in other apps, which "Disk use"
  // would not lead anyone to look for.
  { id: 'disk', label: 'What GitWyrm changed' },
]

/**
 * Agent Setup: the full-screen configuration-sync manager (mockup's
 * `.ag-setup-view`, reached from the sidebar's "Agent setup" tool link).
 *
 * This component is intentionally self-contained and does not touch
 * `AgentDeskView.tsx` or `agentDeskUiStore.ts` (owned by another change in
 * flight at the same time this was written). The mount point it needs:
 * render `<AgentSetupView repoId={repoId} onClose={...} />` as a full-panel
 * replacement for `.ag-main`/`.ag-graph-panel` when the sidebar's "Agent
 * setup" link is clicked, the same `.is-setup` full-screen-takeover shape
 * the mockup uses, and have "Back to chat" call the same `onClose`.
 */
export function AgentSetupView({ repoId, onClose }: { repoId: string | null; onClose: () => void }) {
  const [tab, setTab] = useState<SetupTab>('skills')
  const [activeItem, setActiveItem] = useState<InventoryEntry | null>(null)
  const [batchOpen, setBatchOpen] = useState(false)

  const inventory = useAgentConfigInventory(repoId)
  const detections = useAgentConfigDetectedClients(repoId)

  const entries = inventory.data ?? []
  const differingCount = entries.filter(hasAnyDifference).length

  return (
    // `min-w-0 flex-1`, not just `h-full`: this sits in a flex ROW beside the
    // chat list, and without them it sizes to its own content and draws over
    // the list instead of taking the space left beside it. `min-w-0` is the
    // half that is easy to miss -- a flex child will not shrink below its
    // content width without it, so the wide table pushes it over the sidebar.
    <section
      aria-label="Agent setup manager"
      className="flex h-full min-h-0 min-w-0 flex-1 flex-col overflow-hidden bg-panel"
    >
      <div className="flex flex-none flex-wrap items-start gap-3 border-b border-border px-4.5 py-3">
        <div>
          <div className="text-sm font-semibold text-foreground">Agent setup</div>
          <div className="mt-0.5 text-2xs text-muted-foreground">
            Keep the tools you choose aligned across your agent apps.
          </div>
        </div>
        <Button
          size="sm"
          className="ml-auto"
          onClick={() => setBatchOpen(true)}
          disabled={differingCount === 0}
        >
          <RefreshCw size={13} />
          Match selected apps
        </Button>
        <Button size="sm" variant="secondary" onClick={onClose}>
          Back to chat
        </Button>
      </div>

      <div role="tablist" className="flex flex-none gap-0.5 border-b border-border px-4.5 pt-2">
        {TABS.map((t) => (
          <button
            key={t.id}
            type="button"
            role="tab"
            // A `tablist` whose tabs point at no panel tells a screen reader
            // the tabs exist and nothing about what they control.
            id={`agent-setup-tab-${t.id}`}
            aria-controls="agent-setup-panel"
            aria-selected={tab === t.id}
            onClick={() => setTab(t.id)}
            className={cn(
              'h-[30px] border-b-2 border-transparent px-2.5 text-2xs text-sub transition-colors',
              tab === t.id && 'border-primary text-foreground'
            )}
          >
            {t.label}
          </button>
        ))}
      </div>

      <div
        role="tabpanel"
        id="agent-setup-panel"
        aria-labelledby={`agent-setup-tab-${tab}`}
        className="min-h-0 flex-1 overflow-y-auto p-3.5"
      >
        {/* Disk use is exempt like Detected apps: it reads its own query and
            must not sit behind a scan of the OTHER agent apps' config, which
            answers a different question entirely. */}
        {inventory.isLoading && tab !== 'detected' && tab !== 'disk' ? (
          <p className="py-6 text-center text-2xs text-muted-foreground">Scanning agent apps…</p>
        ) : inventory.isError && tab !== 'detected' && tab !== 'disk' ? (
          // A scan that FAILED used to fall through to the tables, which then
          // said "No skills were found on this machine yet" -- telling someone
          // they have nothing when the truth is that GitWyrm could not look.
          // `AgentCatalog` in the same folder already draws this distinction;
          // this matches it, including the retry.
          <div className="rounded-md border border-destructive/40 bg-destructive/10 p-3">
            <p className="flex items-center gap-1.5 text-2xs font-semibold text-destructive">
              <AlertTriangle size={13} aria-hidden />
              GitWyrm could not look at your other AI apps' settings.
            </p>
            <p className="mt-1 text-2xs text-muted-foreground">
              This is not the same as having none. Nothing has been changed.
            </p>
            <button
              type="button"
              onClick={() => void inventory.refetch()}
              className="mt-2 rounded border border-border px-2 py-1 text-2xs font-semibold hover:bg-panel3"
            >
              Try again
            </button>
          </div>
        ) : tab === 'skills' ? (
          <>
            <SyncSummaryLine entries={entries} kind="skill" />
            <SyncTable entries={entries} kind="skill" onSelectItem={setActiveItem} />
          </>
        ) : tab === 'connections' ? (
          <>
            <SyncSummaryLine entries={entries} kind="mcpConnector" />
            <SyncTable entries={entries} kind="mcpConnector" onSelectItem={setActiveItem} />
          </>
        ) : tab === 'providers' ? (
          <AgentCatalog />
        ) : tab === 'disk' ? (
          // Both are machine-level facts about what GitWyrm has done outside
          // this project: copies it is holding, and settings it has changed.
          <div className="flex flex-col gap-3">
            <AgentCopiesOnDisk />
            <RecentConfigChanges repoId={repoId} />
          </div>
        ) : (
          <DetectedAppsTab detections={detections.data ?? []} />
        )}
      </div>

      {activeItem && (
        <CopyPreviewDialog entry={activeItem} repoId={repoId} onClose={() => setActiveItem(null)} />
      )}
      {batchOpen && (
        <BatchReviewDialog entries={entries} repoId={repoId} onClose={() => setBatchOpen(false)} />
      )}
    </section>
  )
}
