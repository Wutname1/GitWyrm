import { create } from 'zustand'

interface RepoBranchVisibility {
  hidden: string[]
  focused: string | null
}

interface BranchVisibilityState {
  byRepo: Record<string, RepoBranchVisibility>
  hideBranch: (repoId: string, branch: string) => void
  focusBranch: (repoId: string, branch: string) => void
  showBranch: (repoId: string, branch: string, allBranches: string[]) => void
  showAllBranches: (repoId: string) => void
  reconcileBranches: (repoId: string, branches: string[]) => void
}

const EMPTY_VISIBILITY: RepoBranchVisibility = { hidden: [], focused: null }

export function branchVisibilityFor(
  byRepo: Record<string, RepoBranchVisibility>,
  repoId: string | null,
): RepoBranchVisibility {
  return repoId ? (byRepo[repoId] ?? EMPTY_VISIBILITY) : EMPTY_VISIBILITY
}

export function isBranchVisible(visibility: RepoBranchVisibility, branch: string): boolean {
  return visibility.focused != null
    ? visibility.focused === branch
    : !visibility.hidden.includes(branch)
}

export const useBranchVisibilityStore = create<BranchVisibilityState>((set) => ({
  byRepo: {},

  hideBranch: (repoId, branch) =>
    set((state) => {
      const current = branchVisibilityFor(state.byRepo, repoId)
      return {
        byRepo: {
          ...state.byRepo,
          [repoId]: {
            focused: current.focused === branch ? null : current.focused,
            hidden: current.hidden.includes(branch) ? current.hidden : [...current.hidden, branch],
          },
        },
      }
    }),

  focusBranch: (repoId, branch) =>
    set((state) => ({
      byRepo: {
        ...state.byRepo,
        [repoId]: { ...branchVisibilityFor(state.byRepo, repoId), focused: branch },
      },
    })),

  showBranch: (repoId, branch, allBranches) =>
    set((state) => {
      const current = branchVisibilityFor(state.byRepo, repoId)
      // Revealing a second branch while focused turns the focus into an ordinary
      // hidden list. Both requested branches stay visible; everything else stays hidden.
      const hidden = current.focused
        ? allBranches.filter((name) => name !== current.focused && name !== branch)
        : current.hidden.filter((name) => name !== branch)
      return { byRepo: { ...state.byRepo, [repoId]: { hidden, focused: null } } }
    }),

  showAllBranches: (repoId) =>
    set((state) => ({ byRepo: { ...state.byRepo, [repoId]: EMPTY_VISIBILITY } })),

  reconcileBranches: (repoId, branches) =>
    set((state) => {
      const current = branchVisibilityFor(state.byRepo, repoId)
      const known = new Set(branches)
      const hidden = current.hidden.filter((name) => known.has(name))
      const focused = current.focused && known.has(current.focused) ? current.focused : null
      if (hidden.length === current.hidden.length && focused === current.focused) return state
      return { byRepo: { ...state.byRepo, [repoId]: { hidden, focused } } }
    }),
}))
