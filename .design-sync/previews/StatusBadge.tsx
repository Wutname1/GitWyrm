import { StatusBadge } from 'gitwyrm-mockup'

export function AllCodes() {
  return (
    <div style={{ display: 'flex', gap: 10, alignItems: 'center', padding: 20 }}>
      <StatusBadge st="A" />
      <StatusBadge st="M" />
      <StatusBadge st="D" />
      <StatusBadge st="R" />
      <StatusBadge st="!" />
    </div>
  )
}

export function InFileRows() {
  const rows: { st: 'A' | 'M' | 'D' | 'R' | '!'; path: string }[] = [
    { st: 'M', path: 'src/components/domain/graph/Avatar.tsx' },
    { st: 'A', path: 'src/components/domain/NoRepoPlaceholder.tsx' },
    { st: 'D', path: 'src/lib/legacyBindings.ts' },
    { st: 'R', path: 'src/views/GraphView.tsx' },
    { st: '!', path: 'src-tauri/src/settings.rs' },
  ]
  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 6, padding: 20 }}>
      {rows.map((r) => (
        <div key={r.path} style={{ display: 'flex', gap: 8, alignItems: 'center' }}>
          <StatusBadge st={r.st} />
          <span style={{ fontFamily: 'monospace', fontSize: 12, color: 'var(--gw-sub)' }}>
            {r.path}
          </span>
        </div>
      ))}
    </div>
  )
}
