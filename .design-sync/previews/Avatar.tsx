import { Avatar } from 'gitwyrm-mockup'

export function SmallRow() {
  return (
    <div style={{ display: 'flex', gap: 8, alignItems: 'center', padding: 20 }}>
      <Avatar initials="JM" color="#1db584" />
      <Avatar initials="AL" color="#e0a458" />
      <Avatar initials="RK" color="#6ea8fe" />
      <Avatar initials="SP" color="#d16d6d" />
      <Avatar initials="TN" color="#b07de0" />
    </div>
  )
}

export function MediumRow() {
  return (
    <div style={{ display: 'flex', gap: 10, alignItems: 'center', padding: 20 }}>
      <Avatar initials="JM" color="#1db584" size="md" />
      <Avatar initials="AL" color="#e0a458" size="md" />
      <Avatar initials="RK" color="#6ea8fe" size="md" />
      <Avatar initials="SP" color="#d16d6d" size="md" />
    </div>
  )
}

export function InCommitRow() {
  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 10, padding: 20 }}>
      <div style={{ display: 'flex', gap: 8, alignItems: 'center' }}>
        <Avatar initials="JM" color="#1db584" />
        <span style={{ color: 'var(--gw-text)', fontSize: 13 }}>
          fixes: guard secret values before arithmetic
        </span>
      </div>
      <div style={{ display: 'flex', gap: 8, alignItems: 'center' }}>
        <Avatar initials="AL" color="#e0a458" />
        <span style={{ color: 'var(--gw-text)', fontSize: 13 }}>
          improved: preset system supports per-frame styles
        </span>
      </div>
    </div>
  )
}
