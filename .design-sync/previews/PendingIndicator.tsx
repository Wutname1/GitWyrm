import { PendingIndicator, Button } from 'gitwyrm-mockup'

export function Standalone() {
  return (
    <div style={{ padding: 24, display: 'flex', alignItems: 'center', gap: 8 }}>
      <PendingIndicator />
      <span className="text-sm text-muted-foreground">Fetching from origin...</span>
    </div>
  )
}

export function InButton() {
  return (
    <div style={{ padding: 24, display: 'flex', gap: 12, alignItems: 'center' }}>
      <Button disabled aria-busy>
        <PendingIndicator />
        Pushing...
      </Button>
      <Button variant="secondary" disabled aria-busy>
        <PendingIndicator />
        Cloning...
      </Button>
    </div>
  )
}

export function Sizes() {
  return (
    <div style={{ padding: 24, display: 'flex', gap: 16, alignItems: 'center' }}>
      <PendingIndicator className="size-3 text-muted-foreground" />
      <PendingIndicator className="size-4 text-foreground" />
      <PendingIndicator className="size-6 text-primary" />
    </div>
  )
}
