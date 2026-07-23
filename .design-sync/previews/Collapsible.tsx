import {
  Collapsible,
  CollapsibleTrigger,
  CollapsibleContent,
} from 'gitwyrm-mockup'
import { ChevronDown, Archive, GitBranch } from 'lucide-react'

export function StashesSection() {
  return (
    <div style={{ padding: 20, width: 280 }}>
      <Collapsible defaultOpen>
        <CollapsibleTrigger className="flex w-full items-center gap-2 rounded-sm px-2 py-1.5 text-sm font-medium text-foreground hover:bg-accent">
          <ChevronDown style={{ width: 14, height: 14 }} />
          <Archive style={{ width: 14, height: 14 }} />
          Stashes
          <span className="ml-auto text-xs text-muted-foreground">2</span>
        </CollapsibleTrigger>
        <CollapsibleContent className="mt-1 flex flex-col gap-0.5 pl-6 text-sm text-muted-foreground">
          <div className="rounded-sm px-2 py-1 hover:bg-accent hover:text-foreground">
            WIP on main: diff gutter tweaks
          </div>
          <div className="rounded-sm px-2 py-1 hover:bg-accent hover:text-foreground">
            On feature/commit-graph: cache lanes
          </div>
        </CollapsibleContent>
      </Collapsible>
    </div>
  )
}

export function LocalBranches() {
  return (
    <div style={{ padding: 20, width: 280 }}>
      <Collapsible defaultOpen>
        <CollapsibleTrigger className="flex w-full items-center gap-2 rounded-sm px-2 py-1.5 text-sm font-medium text-foreground hover:bg-accent">
          <ChevronDown style={{ width: 14, height: 14 }} />
          <GitBranch style={{ width: 14, height: 14 }} />
          Local branches
          <span className="ml-auto text-xs text-muted-foreground">3</span>
        </CollapsibleTrigger>
        <CollapsibleContent className="mt-1 flex flex-col gap-0.5 pl-6 text-sm text-muted-foreground">
          <div className="rounded-sm px-2 py-1 font-medium text-foreground">main</div>
          <div className="rounded-sm px-2 py-1">feature/commit-graph</div>
          <div className="rounded-sm px-2 py-1">fix/rebase-conflict</div>
        </CollapsibleContent>
      </Collapsible>
    </div>
  )
}
