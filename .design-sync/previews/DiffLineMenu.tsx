import { DiffLineMenu } from 'gitwyrm-mockup'

// DiffLineMenu wraps its children in a right-click ContextMenu. Radix
// ContextMenu can only open on a real right-click (no defaultOpen), so the
// menu content cannot be shown statically. These stories render the trigger
// row -- the visible surface the user right-clicks. Noted as a floor
// candidate for the menu content in learnings.

function DiffLine({ sign, text }: { sign: '+' | '-' | ' '; text: string }) {
  const color = sign === '+' ? 'var(--gw-green)' : sign === '-' ? 'var(--gw-red)' : 'var(--gw-sub)'
  return (
    <span
      style={{
        display: 'block',
        fontFamily: 'monospace',
        fontSize: 12,
        whiteSpace: 'pre',
        color,
        padding: '1px 8px',
      }}
    >
      {sign} {text}
    </span>
  )
}

export function UnstagedLine() {
  return (
    <div style={{ padding: 20 }}>
      <DiffLineMenu
        kind="unstaged"
        count={1}
        onOpenChange={() => {}}
        onApply={() => {}}
        onDiscard={() => {}}
      >
        <DiffLine sign="+" text="const dnd = useRefDnd(self)" />
      </DiffLineMenu>
    </div>
  )
}

export function StagedSelection() {
  return (
    <div style={{ padding: 20 }}>
      <DiffLineMenu
        kind="staged"
        count={5}
        onOpenChange={() => {}}
        onApply={() => {}}
        onDiscard={() => {}}
      >
        <div>
          <DiffLine sign="-" text="if (status > 0) {" />
          <DiffLine sign="+" text="if (status && canaccessvalue(status)) {" />
        </div>
      </DiffLineMenu>
    </div>
  )
}
