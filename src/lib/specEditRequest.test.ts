import { describe, it, expect } from 'vitest'
import { detectEditRequest } from './specEditRequest'

const DELTAS = ['specs/ai-ask/spec.md', 'specs/spec-desk/spec.md']

describe('detectEditRequest', () => {
  it('offers to edit the proposal when asked to reword it', () => {
    expect(detectEditRequest('Reword the proposal to be shorter')).toEqual({
      file: 'proposal.md',
      instruction: 'Reword the proposal to be shorter',
    })
  })

  it('recognises the proposal by its section names', () => {
    // People say "the Why", not "proposal.md".
    expect(detectEditRequest('Clarify the Why section')?.file).toBe('proposal.md')
    expect(detectEditRequest('Rewrite the Impact')?.file).toBe('proposal.md')
  })

  it('recognises tasks and design', () => {
    expect(detectEditRequest('Add a task for the migration')?.file).toBe('tasks.md')
    expect(detectEditRequest('Expand the design notes')?.file).toBe('design.md')
  })

  it('names a delta file when the question does', () => {
    expect(detectEditRequest('Reword specs/ai-ask/spec.md', DELTAS)?.file).toBe(
      'specs/ai-ask/spec.md'
    )
  })

  it('recognises a delta by its capability folder', () => {
    // "the spec-desk spec" is how these are referred to in conversation.
    expect(detectEditRequest('Tidy up the spec-desk spec', DELTAS)?.file).toBe(
      'specs/spec-desk/spec.md'
    )
  })

  it('stays silent for an ordinary question about a file', () => {
    // Naming a file is not asking for it to be changed.
    expect(detectEditRequest('What does tasks.md say?')).toBeNull()
    expect(detectEditRequest('Why is the proposal written this way?')).toBeNull()
    expect(detectEditRequest('Which files does this change touch?')).toBeNull()
  })

  it('stays silent when no file is named', () => {
    // The change has four files; guessing which one is worse than not offering.
    expect(detectEditRequest('Reword this')).toBeNull()
    expect(detectEditRequest('Clean it up')).toBeNull()
  })

  it('passes the question through as the instruction', () => {
    const q = 'Shorten the Why section to two sentences'
    expect(detectEditRequest(q)?.instruction).toBe(q)
  })

  it('does not match a delta by the bare filename spec.md', () => {
    // Every delta is called spec.md, so it identifies nothing on its own.
    expect(detectEditRequest('Reword spec.md', DELTAS)?.file).not.toBe('specs/ai-ask/spec.md')
  })
})
