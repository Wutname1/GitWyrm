import { Input } from 'gitwyrm-mockup'

export function Default() {
  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 12, maxWidth: 360, padding: 20 }}>
      <Input placeholder="Search branches..." />
      <Input defaultValue="feature/commit-graph" />
    </div>
  )
}

export function States() {
  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 12, maxWidth: 360, padding: 20 }}>
      <Input placeholder="Remote name (e.g. origin)" />
      <Input defaultValue="origin" disabled />
      <Input defaultValue="not-a-valid-url" aria-invalid />
    </div>
  )
}

export function Types() {
  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 12, maxWidth: 360, padding: 20 }}>
      <Input type="text" placeholder="Branch name" />
      <Input type="password" defaultValue="ghp_secrettoken" />
      <Input type="search" placeholder="Filter commits" />
    </div>
  )
}
