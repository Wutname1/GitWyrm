import { GithubIcon } from 'gitwyrm-mockup'

export function Sizes() {
  return (
    <div
      style={{ display: 'flex', gap: 16, alignItems: 'center', padding: 20, color: '#d7e0ea' }}
    >
      <GithubIcon size={14} />
      <GithubIcon size={20} />
      <GithubIcon size={28} />
      <GithubIcon size={40} />
    </div>
  )
}

export function InButton() {
  return (
    <div style={{ padding: 20 }}>
      <div
        style={{
          display: 'inline-flex',
          gap: 8,
          alignItems: 'center',
          padding: '6px 12px',
          borderRadius: 6,
          border: '1px solid var(--gw-border)',
          color: '#d7e0ea',
          fontSize: 13,
        }}
      >
        <GithubIcon size={16} />
        Connect to GitHub
      </div>
    </div>
  )
}
