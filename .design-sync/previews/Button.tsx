import { Button } from 'gitwyrm-mockup'
import { GitBranch, Plus, RefreshCw, Trash2 } from 'lucide-react'

export function Variants() {
  return (
    <div style={{ display: 'flex', flexWrap: 'wrap', gap: 10, alignItems: 'center', padding: 20 }}>
      <Button variant="default">Commit</Button>
      <Button variant="secondary">Stage all</Button>
      <Button variant="outline">Discard</Button>
      <Button variant="ghost">Cancel</Button>
      <Button variant="destructive">Delete branch</Button>
      <Button variant="link">View on GitHub</Button>
    </div>
  )
}

export function Sizes() {
  return (
    <div style={{ display: 'flex', flexWrap: 'wrap', gap: 10, alignItems: 'center', padding: 20 }}>
      <Button size="xs">Extra small</Button>
      <Button size="sm">Small</Button>
      <Button size="default">Default</Button>
      <Button size="lg">Large</Button>
    </div>
  )
}

export function WithIcons() {
  return (
    <div style={{ display: 'flex', flexWrap: 'wrap', gap: 10, alignItems: 'center', padding: 20 }}>
      <Button>
        <GitBranch /> New branch
      </Button>
      <Button variant="secondary">
        <Plus /> Add remote
      </Button>
      <Button variant="outline">
        <RefreshCw /> Fetch
      </Button>
      <Button variant="destructive">
        <Trash2 /> Delete
      </Button>
    </div>
  )
}

export function IconButtons() {
  return (
    <div style={{ display: 'flex', flexWrap: 'wrap', gap: 10, alignItems: 'center', padding: 20 }}>
      <Button size="icon-sm" variant="ghost" aria-label="Fetch">
        <RefreshCw />
      </Button>
      <Button size="icon" variant="outline" aria-label="New branch">
        <GitBranch />
      </Button>
      <Button size="icon-lg" variant="default" aria-label="Add">
        <Plus />
      </Button>
    </div>
  )
}

export function Disabled() {
  return (
    <div style={{ display: 'flex', flexWrap: 'wrap', gap: 10, alignItems: 'center', padding: 20 }}>
      <Button disabled>Commit</Button>
      <Button variant="outline" disabled>
        Discard
      </Button>
      <Button variant="destructive" disabled>
        Delete
      </Button>
    </div>
  )
}
