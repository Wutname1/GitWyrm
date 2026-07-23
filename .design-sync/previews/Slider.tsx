import { Slider } from 'gitwyrm-mockup'

export function GraphSpacing() {
  return (
    <div style={{ padding: 24, display: 'flex', flexDirection: 'column', gap: 8, width: 320 }}>
      <div className="flex items-center justify-between text-xs text-foreground">
        <span>Graph row spacing</span>
        <span className="text-muted-foreground">18 px</span>
      </div>
      <Slider defaultValue={[18]} min={12} max={40} step={1} />
    </div>
  )
}

export function FontSize() {
  return (
    <div style={{ padding: 24, display: 'flex', flexDirection: 'column', gap: 8, width: 320 }}>
      <div className="flex items-center justify-between text-xs text-foreground">
        <span>Diff font size</span>
        <span className="text-muted-foreground">13 px</span>
      </div>
      <Slider defaultValue={[13]} min={10} max={20} step={1} />
    </div>
  )
}

export function CommitLimit() {
  return (
    <div style={{ padding: 24, display: 'flex', flexDirection: 'column', gap: 8, width: 320 }}>
      <div className="flex items-center justify-between text-xs text-foreground">
        <span>Commits loaded per page</span>
        <span className="text-muted-foreground">500</span>
      </div>
      <Slider defaultValue={[500]} min={100} max={2000} step={100} />
    </div>
  )
}

export function Disabled() {
  return (
    <div style={{ padding: 24, display: 'flex', flexDirection: 'column', gap: 8, width: 320 }}>
      <div className="flex items-center justify-between text-xs text-foreground">
        <span>Blame heat range (needs history)</span>
        <span className="text-muted-foreground">30 days</span>
      </div>
      <Slider defaultValue={[30]} min={1} max={90} disabled />
    </div>
  )
}
