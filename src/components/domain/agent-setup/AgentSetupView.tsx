import { useState } from 'react'
import { RefreshCw } from 'lucide-react'
import type { InventoryEntry } from '@/lib/bindings'
import { useAgentConfigDetectedClients, useAgentConfigInventory } from '@/hooks/useAgentConfig'
import { hasAnyDifference } from '@/lib/agentConfig'
import { Button } from '@/components/ui/button'
import { cn } from '@/lib/utils'
import { SyncSummaryLine, SyncTable } from './SyncTable'
import { CopyPreviewDialog } from './CopyPreviewDialog'
import { BatchReviewDialog } from './BatchReviewDialog'
import { DetectedAppsTab } from './DetectedAppsTab'

type SetupTab = 'skills' | 'connections' | 'providers' | 'detected'

const TABS: { id: SetupTab; label: string }[] = [
  { id: 'skills', label: 'Skills' },
  { id: 'connections', label: 'Connections' },
  { id: 'providers', label: 'Providers' },
  { id: 'detected', label: 'Detected apps' },
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
    <section aria-label="Agent setup manager" className="flex h-full min-h-0 flex-col bg-panel">
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

      <div className="min-h-0 flex-1 overflow-y-auto p-3.5">
        {inventory.isLoading && tab !== 'detected' ? (
          <p className="py-6 text-center text-2xs text-muted-foreground">Scanning agent apps…</p>
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
          <p className="py-6 text-center text-2xs text-muted-foreground">
            Provider configuration sync is not available yet. Skills and MCP connectors can be
            compared and copied today; provider credentials are kept out of scope until a safe,
            independently proven writer exists for them.
          </p>
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
