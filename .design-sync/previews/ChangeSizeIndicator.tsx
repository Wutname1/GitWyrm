import { ChangeSizeIndicator } from 'gitwyrm-mockup'

export function SmallDiff() {
  return (
    <div style={{ padding: 20 }}>
      <ChangeSizeIndicator
        filesChanged={2}
        additions={14}
        deletions={3}
        showLineCounts
        mode="row"
      />
    </div>
  )
}

export function LargeDiff() {
  return (
    <div style={{ padding: 20 }}>
      <ChangeSizeIndicator
        filesChanged={37}
        additions={1284}
        deletions={562}
        showLineCounts
        mode="row"
      />
    </div>
  )
}

export function ColumnMode() {
  return (
    <div style={{ padding: 20 }}>
      <ChangeSizeIndicator
        filesChanged={9}
        additions={210}
        deletions={48}
        showLineCounts
        mode="column"
      />
    </div>
  )
}

export function BarOnly() {
  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 10, padding: 20 }}>
      <ChangeSizeIndicator
        filesChanged={4}
        additions={88}
        deletions={0}
        showLineCounts={false}
        mode="row"
      />
      <ChangeSizeIndicator
        filesChanged={1}
        additions={0}
        deletions={26}
        showLineCounts={false}
        mode="row"
      />
    </div>
  )
}
