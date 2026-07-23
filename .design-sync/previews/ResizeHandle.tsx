import { ResizeHandle } from 'gitwyrm-mockup'
import { GitBranch, GitCommitHorizontal } from 'lucide-react'

export function BetweenPanels() {
  return (
    <div style={{ padding: 24 }}>
      <div className="relative flex h-56 w-[420px] overflow-hidden rounded-md border border-border">
        <aside className="relative w-40 shrink-0 border-r border-border bg-panel2 p-3">
          <div className="text-xs font-medium uppercase tracking-wide text-muted-foreground">
            Branches
          </div>
          <div className="mt-2 flex items-center gap-2 text-sm text-foreground">
            <GitBranch className="size-3.5 text-muted-foreground" /> main
          </div>
          <div className="mt-1 flex items-center gap-2 text-sm text-foreground">
            <GitBranch className="size-3.5 text-muted-foreground" /> feature/rebase-ui
          </div>
          <ResizeHandle
            ariaLabel="Resize sidebar"
            min={120}
            max={280}
            defaultValue={160}
            onChange={() => {}}
            className="right-0"
          />
        </aside>
        <main className="flex-1 p-3">
          <div className="flex items-center gap-2 text-sm text-foreground">
            <GitCommitHorizontal className="size-3.5 text-muted-foreground" />
            <span className="font-mono text-xs text-muted-foreground">a1c9f42</span>
            Fix detached HEAD crash
          </div>
        </main>
      </div>
    </div>
  )
}

export function Active() {
  return (
    <div style={{ padding: 24 }}>
      <div className="relative flex h-40 w-[360px] overflow-hidden rounded-md border border-border">
        <div className="relative w-44 shrink-0 border-r border-border bg-panel2 p-3 text-sm text-foreground">
          Left pane
          <ResizeHandle
            ariaLabel="Resize"
            min={100}
            max={260}
            defaultValue={176}
            onChange={() => {}}
            className="right-0 after:bg-primary"
          />
        </div>
        <div className="flex-1 p-3 text-sm text-muted-foreground">Right pane</div>
      </div>
    </div>
  )
}
