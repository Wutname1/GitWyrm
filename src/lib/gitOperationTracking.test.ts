import { describe, expect, it } from 'vitest'
import {
  beginGitOperation,
  isGitOperationInFlight,
  noteRefreshSkipped,
  onSkippedRefreshDue,
} from './queryKeys'

describe('git operation tracking', () => {
  it('stays busy until every overlapping operation has finished', () => {
    const endPush = beginGitOperation('overlap')
    const endFetch = beginGitOperation('overlap')
    endPush()
    expect(isGitOperationInFlight('overlap')).toBe(true)
    endFetch()
    expect(isGitOperationInFlight('overlap')).toBe(false)
  })

  it('ignores a release called twice', () => {
    const endA = beginGitOperation('twice')
    const endB = beginGitOperation('twice')
    endA()
    endA()
    expect(isGitOperationInFlight('twice')).toBe(true)
    endB()
    expect(isGitOperationInFlight('twice')).toBe(false)
  })

  // A commit made by another program while a push runs is held back by the
  // watcher. It has to land once the push settles, or the graph and the push
  // count stay behind the changes list.
  it('replays a held-back refresh once the repo goes idle', () => {
    const seen: string[] = []
    const stop = onSkippedRefreshDue((id) => seen.push(id))
    const end = beginGitOperation('replay')
    noteRefreshSkipped('replay')
    expect(seen).toEqual([])
    end()
    expect(seen).toEqual(['replay'])
    stop()
  })

  it('does not replay when nothing was held back', () => {
    const seen: string[] = []
    const stop = onSkippedRefreshDue((id) => seen.push(id))
    const end = beginGitOperation('quiet')
    end()
    expect(seen).toEqual([])
    stop()
  })
})
