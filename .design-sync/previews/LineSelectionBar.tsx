import { LineSelectionBar } from 'gitwyrm-mockup'

export function Unstaged() {
  return (
    <div style={{ padding: 20 }}>
      <LineSelectionBar count={12} kind="unstaged" onApply={() => {}} onClear={() => {}} />
    </div>
  )
}

export function UnstagedWithDiscard() {
  return (
    <div style={{ padding: 20 }}>
      <LineSelectionBar
        count={7}
        kind="unstaged"
        onApply={() => {}}
        onDiscard={() => {}}
        onClear={() => {}}
      />
    </div>
  )
}

export function Staged() {
  return (
    <div style={{ padding: 20 }}>
      <LineSelectionBar count={3} kind="staged" onApply={() => {}} onClear={() => {}} />
    </div>
  )
}

export function SingleLine() {
  return (
    <div style={{ padding: 20 }}>
      <LineSelectionBar count={1} kind="unstaged" onApply={() => {}} onClear={() => {}} />
    </div>
  )
}
