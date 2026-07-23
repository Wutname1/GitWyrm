import { Textarea } from 'gitwyrm-mockup'

export function CommitMessage() {
  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 12, maxWidth: 460, padding: 20 }}>
      <Textarea
        rows={5}
        defaultValue={
          'Fix stale ref badges after force-push\n\nThe ref cache was keyed on the old commit id, so a force-push left\nthe badge pointing at a commit that no longer existed. Re-key on\nfetch and drop entries whose target is gone.'
        }
      />
    </div>
  )
}

export function Placeholder() {
  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 12, maxWidth: 460, padding: 20 }}>
      <Textarea rows={4} placeholder="Describe what changed and why..." />
    </div>
  )
}

export function Disabled() {
  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 12, maxWidth: 460, padding: 20 }}>
      <Textarea rows={3} disabled defaultValue="Merge branch 'main' into feature/graph" />
    </div>
  )
}
