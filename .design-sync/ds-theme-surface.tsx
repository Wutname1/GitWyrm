import * as React from 'react'
export function GwThemeSurface({ children }: { children?: React.ReactNode }) {
  return (
    <div style={{ background: 'var(--gw-bg, #0b0f14)', color: 'var(--gw-text, #d7e0ea)', minHeight: '100%', margin: '-24px', padding: '24px' }}>
      {children}
    </div>
  )
}
