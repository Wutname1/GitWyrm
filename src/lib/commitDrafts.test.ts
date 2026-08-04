import { describe, it, expect } from 'vitest'
import { applyDraft, clearDraft, type CommitDraftMap } from './commitDrafts'

const REPO_A = 'repo-a'
const REPO_B = 'repo-b'

/**
 * The bug these guard against: the commit box kept its message in component
 * state, and one instance was reused across repo tabs. Staging in one repo,
 * starting an AI generation, then switching tabs carried the spinner over and
 * dropped the finished message into whichever repo happened to be on screen.
 *
 * Drafts are keyed by repo so a message can only ever reach the repo it was
 * written for.
 */
describe('commit drafts', () => {
  it("keeps each repo's draft separate", () => {
    let drafts: CommitDraftMap = {}
    drafts = applyDraft(drafts, REPO_A, { summary: 'fixes: thing in A' })
    drafts = applyDraft(drafts, REPO_B, { summary: 'new: thing in B' })

    expect(drafts[REPO_A]?.summary).toBe('fixes: thing in A')
    expect(drafts[REPO_B]?.summary).toBe('new: thing in B')
  })

  it('merges partial edits instead of replacing the draft', () => {
    let drafts: CommitDraftMap = {}
    drafts = applyDraft(drafts, REPO_A, { summary: 'summary' })
    drafts = applyDraft(drafts, REPO_A, { description: 'body' })

    // Typing in the description box must not wipe the summary above it.
    expect(drafts[REPO_A]).toMatchObject({
      summary: 'summary',
      description: 'body',
    })
  })

  it('lands a generated message on its own repo, not the active one', () => {
    // Generation starts on A; the user switches to B before it finishes.
    let drafts: CommitDraftMap = applyDraft(({} as CommitDraftMap), REPO_B, {
      summary: 'typed in B',
    })
    drafts = applyDraft(drafts, REPO_A, {
      summary: 'generated for A',
      description: 'body',
    })

    expect(drafts[REPO_A]?.summary).toBe('generated for A')
    // B keeps what the user typed; the generated message does not land here.
    expect(drafts[REPO_B]?.summary).toBe('typed in B')
  })

  it('forgets a draft once its commit is written', () => {
    const drafts = applyDraft({}, REPO_A, { summary: 'x' })
    expect(clearDraft(drafts, REPO_A)[REPO_A]).toBeUndefined()
  })

  it('leaves other repos alone when one draft is cleared', () => {
    let drafts: CommitDraftMap = {}
    drafts = applyDraft(drafts, REPO_A, { summary: 'a' })
    drafts = applyDraft(drafts, REPO_B, { summary: 'b' })

    expect(clearDraft(drafts, REPO_A)[REPO_B]?.summary).toBe('b')
  })

  it('keeps no entry for a draft edited back to empty', () => {
    let drafts: CommitDraftMap = applyDraft({}, REPO_A, {
      summary: 'typed then deleted',
    })
    drafts = applyDraft(drafts, REPO_A, { summary: '' })

    expect(drafts[REPO_A]).toBeUndefined()
  })

  it('keeps an amend-only draft, which has no text yet', () => {
    // Ticking amend before the previous commit has loaded leaves both text
    // fields blank; dropping it here would silently untick the box.
    const drafts = applyDraft({}, REPO_A, { amend: true })
    expect(drafts[REPO_A]?.amend).toBe(true)
  })

  it('returns the same map when clearing a draft that is not there', () => {
    const drafts = applyDraft({}, REPO_A, { summary: 'a' })
    // Identity, so subscribers do not re-render on a no-op.
    expect(clearDraft(drafts, REPO_B)).toBe(drafts)
  })
})
