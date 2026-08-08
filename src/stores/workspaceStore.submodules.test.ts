import { describe, it, expect, beforeEach } from 'vitest'
import type { RepoInfo } from '@/lib/bindings'

// The tests run in the `node` environment (see vitest.config.ts). Importing the
// store pulls in settingsSync, which reads `window` at module scope to decide
// whether it is inside Tauri. An empty object answers that question correctly --
// it is not -- and is cheaper than pulling in a whole DOM for state assertions.
;(globalThis as { window?: unknown }).window ??= {}

const { isSubmoduleGroup, useWorkspaceStore } = await import('./workspaceStore')

const PARENT = 'C:\\code\\parent'
const SUB = 'C:\\code\\parent\\vendor\\lib'
const SUB2 = 'C:\\code\\parent\\vendor\\other'
const LOOSE = 'C:\\code\\loose'

function repo(id: string, path: string): RepoInfo {
  return { id, path, name: path.split('\\').pop()! } as RepoInfo
}

const parent = repo('r-parent', PARENT)
const sub = repo('r-sub', SUB)
const sub2 = repo('r-sub2', SUB2)
const loose = repo('r-loose', LOOSE)

/**
 * `hydrated: false` keeps `schedulePersist` from reaching for Tauri: it returns
 * early until the store has loaded, so these run without a backend.
 */
beforeEach(() => {
  useWorkspaceStore.setState({
    openRepos: [],
    activeRepoId: null,
    tabGroups: [],
    tabOrder: [],
    pinnedTabPaths: [],
    savedTabGroups: [],
    recents: [],
    commitDrafts: {},
    hydrated: false,
  })
})

/** The group a path currently sits in, if any. */
function groupOf(path: string) {
  return useWorkspaceStore
    .getState()
    .tabGroups.find((g) =>
      g.repoPaths.some((p) => p.toLowerCase() === path.toLowerCase())
    )
}

describe('submodule tabs', () => {
  it('nests the submodule in a group owned by its project', () => {
    const s = useWorkspaceStore.getState()
    s.addRepo(parent)
    s.addSubmoduleRepo(sub, PARENT)

    const group = groupOf(SUB)
    expect(group).toBeDefined()
    expect(isSubmoduleGroup(group!)).toBe(true)
    expect(group!.parentPath).toBe(PARENT)
    // The project joins the group as its first member: it is the row the
    // submodules attach under, not a separate tab above them.
    expect(groupOf(PARENT)?.id).toBe(group!.id)
    expect(group!.repoPaths[0]!.toLowerCase()).toBe(PARENT.toLowerCase())
    // ...and it is no longer a loose tab in its own right, or it would draw twice.
    expect(
      useWorkspaceStore
        .getState()
        .tabOrder.some(
          (i) => i.type === 'repo' && i.path.toLowerCase() === PARENT.toLowerCase()
        )
    ).toBe(false)
  })

  it('takes the slot its project held in the order', () => {
    const s = useWorkspaceStore.getState()
    s.addRepo(parent)
    s.addRepo(loose)
    s.addSubmoduleRepo(sub, PARENT)

    // The project was first, so the group that now contains it is first too --
    // opening a submodule must not shunt the project to the end of the strip.
    const order = useWorkspaceStore.getState().tabOrder
    expect(order[0]).toEqual({
      type: 'group',
      id: groupOf(SUB)!.id,
    })
    expect(order).toHaveLength(2)
  })

  it('reuses one group for every submodule of the same project', () => {
    const s = useWorkspaceStore.getState()
    s.addRepo(parent)
    s.addSubmoduleRepo(sub, PARENT)
    s.addSubmoduleRepo(sub2, PARENT)

    const groups = useWorkspaceStore.getState().tabGroups
    expect(groups).toHaveLength(1)
    // The project plus both submodules.
    expect(groups[0]!.repoPaths).toHaveLength(3)
  })

  it('opens as an ordinary tab when the project is not open', () => {
    const s = useWorkspaceStore.getState()
    s.addSubmoduleRepo(sub, PARENT)

    // Nothing to attach to, so nothing is added rather than a group with no
    // project above it.
    expect(useWorkspaceStore.getState().tabGroups).toHaveLength(0)
    expect(useWorkspaceStore.getState().openRepos).toHaveLength(0)
  })

  describe('cannot be pulled out of its project', () => {
    beforeEach(() => {
      const s = useWorkspaceStore.getState()
      s.addRepo(parent)
      s.addRepo(loose)
      s.addSubmoduleRepo(sub, PARENT)
    })

    it('refuses to pin', () => {
      useWorkspaceStore.getState().toggleTabPin(SUB)
      expect(useWorkspaceStore.getState().pinnedTabPaths).toHaveLength(0)
      expect(groupOf(SUB)).toBeDefined()
    })

    it('refuses to be removed from its group', () => {
      useWorkspaceStore.getState().removeRepoFromGroup(SUB)
      expect(groupOf(SUB)).toBeDefined()
    })

    it('refuses to join a group the user built', () => {
      const id = useWorkspaceStore.getState().createTabGroup([LOOSE])
      useWorkspaceStore.getState().addRepoToGroup(SUB, id)
      expect(groupOf(SUB)!.parentPath).toBe(PARENT)
    })

    it('is left out when a new group is built from it', () => {
      useWorkspaceStore.getState().createTabGroup([LOOSE, SUB])
      // The loose tab grouped on its own; the submodule stayed put.
      expect(groupOf(SUB)!.parentPath).toBe(PARENT)
      expect(groupOf(LOOSE)!.parentPath).toBeUndefined()
    })

    it('refuses to be dragged elsewhere in the order', () => {
      useWorkspaceStore.getState().moveRepoToOrder(SUB, 0)
      expect(groupOf(SUB)).toBeDefined()
      useWorkspaceStore.getState().moveRepoBeside(SUB, LOOSE, 'before')
      expect(groupOf(SUB)).toBeDefined()
    })

    it('refuses to have another tab dropped in beside it', () => {
      useWorkspaceStore.getState().moveRepoBeside(LOOSE, SUB, 'after')
      expect(groupOf(LOOSE)).toBeUndefined()
      // Still just the project and its one submodule.
      expect(groupOf(SUB)!.repoPaths).toHaveLength(2)
    })

    it('cannot have its group ungrouped or saved', () => {
      const id = groupOf(SUB)!.id
      useWorkspaceStore.getState().ungroupTabGroup(id)
      expect(groupOf(SUB)).toBeDefined()
      useWorkspaceStore.getState().saveTabGroup(id)
      expect(useWorkspaceStore.getState().savedTabGroups).toHaveLength(0)
    })
  })

  it('closes its submodules when the project closes', () => {
    const s = useWorkspaceStore.getState()
    s.addRepo(loose)
    s.addRepo(parent)
    s.addSubmoduleRepo(sub, PARENT)
    s.addSubmoduleRepo(sub2, PARENT)

    useWorkspaceStore.getState().removeRepo(parent.id)

    const after = useWorkspaceStore.getState()
    expect(after.openRepos.map((r) => r.id)).toEqual([loose.id])
    expect(after.tabGroups).toHaveLength(0)
    // The active tab was a submodule that just closed, so focus lands on what
    // is left rather than a repo that is gone.
    expect(after.activeRepoId).toBe(loose.id)
  })

  it('keeps the project open when a submodule closes', () => {
    const s = useWorkspaceStore.getState()
    s.addRepo(parent)
    s.addSubmoduleRepo(sub, PARENT)
    s.addSubmoduleRepo(sub2, PARENT)

    useWorkspaceStore.getState().removeRepo(sub.id)

    const after = useWorkspaceStore.getState()
    expect(after.openRepos.map((r) => r.id).sort()).toEqual(
      [parent.id, sub2.id].sort()
    )
    expect(groupOf(SUB2)).toBeDefined()
  })

  it('drops the group when the last submodule closes', () => {
    const s = useWorkspaceStore.getState()
    s.addRepo(parent)
    s.addSubmoduleRepo(sub, PARENT)

    useWorkspaceStore.getState().removeRepo(sub.id)

    // With nothing left nested under it, the project goes back to being an
    // ordinary tab rather than a group wrapping one member.
    expect(useWorkspaceStore.getState().tabGroups).toHaveLength(0)
    expect(useWorkspaceStore.getState().openRepos.map((r) => r.id)).toEqual([
      parent.id,
    ])
    expect(useWorkspaceStore.getState().tabOrder).toEqual([
      { type: 'repo', path: PARENT },
    ])
  })

  it('drops a group whose project failed to reopen', () => {
    // Restored settings can name a project that is no longer on disk. Its
    // submodules still open, and must not nest under a tab that is not there.
    useWorkspaceStore.setState({
      openRepos: [sub],
      tabGroups: [
        {
          id: 'subs',
          name: 'parent',
          color: '#000',
          collapsed: false,
          repoPaths: [SUB],
          parentPath: PARENT,
        },
      ],
      tabOrder: [{ type: 'group', id: 'subs' }],
    })

    useWorkspaceStore.getState().finishRepoRestore()

    const after = useWorkspaceStore.getState()
    expect(after.tabGroups).toHaveLength(0)
    // Falls back to an ordinary tab rather than vanishing from the strip.
    expect(after.tabOrder).toEqual([{ type: 'repo', path: SUB }])
  })
})
