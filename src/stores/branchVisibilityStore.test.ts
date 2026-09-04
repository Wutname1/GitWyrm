import { beforeEach, describe, expect, it } from 'vitest'
import {
  branchVisibilityFor,
  isBranchVisible,
  useBranchVisibilityStore,
} from './branchVisibilityStore'

describe('branch visibility', () => {
  beforeEach(() => useBranchVisibilityStore.setState({ byRepo: {} }))

  it('focuses one branch and can reveal a second without revealing every branch', () => {
    const store = useBranchVisibilityStore.getState()
    store.focusBranch('repo', 'main')

    let visibility = branchVisibilityFor(useBranchVisibilityStore.getState().byRepo, 'repo')
    expect(isBranchVisible(visibility, 'main')).toBe(true)
    expect(isBranchVisible(visibility, 'feature')).toBe(false)

    useBranchVisibilityStore.getState().showBranch(
      'repo',
      'feature',
      ['main', 'feature', 'old'],
    )
    visibility = branchVisibilityFor(useBranchVisibilityStore.getState().byRepo, 'repo')
    expect(visibility.focused).toBeNull()
    expect(visibility.hidden).toEqual(['old'])
  })

  it('drops filters for branches that no longer exist', () => {
    const store = useBranchVisibilityStore.getState()
    store.hideBranch('repo', 'deleted')
    store.focusBranch('repo', 'renamed')
    useBranchVisibilityStore.getState().reconcileBranches('repo', ['main'])

    expect(branchVisibilityFor(useBranchVisibilityStore.getState().byRepo, 'repo')).toEqual({
      hidden: [],
      focused: null,
    })
  })
})
