import { Separator } from 'gitwyrm-mockup'

export function BetweenSections() {
  return (
    <div style={{ padding: 24, width: 320 }} className="text-sm text-foreground">
      <div className="text-xs font-medium uppercase tracking-wide text-muted-foreground">
        Local branches
      </div>
      <div className="py-2">main</div>
      <div className="py-2">feature/commit-graph</div>
      <Separator className="my-3" />
      <div className="text-xs font-medium uppercase tracking-wide text-muted-foreground">
        Remotes
      </div>
      <div className="py-2">origin</div>
      <div className="py-2">upstream</div>
    </div>
  )
}

export function VerticalInToolbar() {
  return (
    <div style={{ padding: 24 }}>
      <div className="flex h-8 items-center gap-3 text-sm text-foreground">
        <span>Fetch</span>
        <Separator orientation="vertical" />
        <span>Pull</span>
        <Separator orientation="vertical" />
        <span>Push</span>
        <Separator orientation="vertical" />
        <span className="text-muted-foreground">origin/main</span>
      </div>
    </div>
  )
}
