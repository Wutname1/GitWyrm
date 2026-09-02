import { describe, expect, it } from 'vitest'
import { incomingName, sideNames } from './conflictSides'
import type { MergeState } from '@/lib/bindings'

const state = (over: Partial<MergeState>): MergeState => ({
  merging: true,
  operation: 'Merge',
  incoming_label: null,
  full_message: null,
  conflicts: [],
  ...over,
})

describe('incomingName', () => {
  it('pulls the branch out of git\'s own merge wording', () => {
    expect(incomingName("Merge branch 'main'")).toBe('main')
    expect(incomingName("Merge remote-tracking branch 'origin/main'")).toBe('origin/main')
  })

  it('passes through a label that is already a bare name', () => {
    expect(incomingName('some commit subject')).toBe('some commit subject')
  })

  it('is empty when there is no label', () => {
    expect(incomingName(null)).toBe('')
  })
})

describe('sideNames', () => {
  it('names ours for the current branch and theirs for the incoming one', () => {
    const s = sideNames(state({ incoming_label: "Merge branch 'main'" }), 'agent-desk')
    expect(s).toEqual({ ours: 'agent-desk', theirs: 'main', named: true })
  })

  // A rebase replays YOUR commits onto the other branch, so git's stage 2
  // ("ours") holds the branch being replayed onto -- the opposite of a merge.
  // Verified against a real rebase; labelling these the merge way tells the
  // user to keep exactly the wrong side.
  it('does not call the current branch "ours" during a rebase', () => {
    const s = sideNames(state({ operation: 'Rebase', incoming_label: 'feature' }), 'feature')
    expect(s.ours).not.toBe('feature')
    expect(s.theirs).toBe('feature')
    expect(s.named).toBe(false)
  })

  it('falls back to plain wording when a name is missing', () => {
    const s = sideNames(state({ incoming_label: null }), null)
    expect(s).toEqual({ ours: 'your version', theirs: 'incoming version', named: false })
  })
})
