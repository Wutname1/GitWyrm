import { describe, it, expect, beforeEach } from 'vitest'
import { changeDraftPrefix, draftKey, useSpecDraftStore } from './specDraftStore'

const REPO = 'repo-1'
const CHANGE = 'add-thing'

beforeEach(() => {
  useSpecDraftStore.setState({ drafts: {} })
})

describe('specDraftStore', () => {
  /**
   * The bug this guards against: the store wrote drafts under a key built one
   * way while the component subscribed with a key built another, so every
   * lookup missed and the editor opened empty with the text sitting in the
   * store the whole time. Nothing threw, and the file read fine -- the only
   * symptom was a blank editor.
   *
   * Any component subscribing to one draft must use `draftKey`. These assert
   * the exported helpers really are what the store stores under.
   */
  it('stores drafts under the exported key', () => {
    useSpecDraftStore.getState().open(REPO, CHANGE, 'proposal.md', '# Change\n')
    const key = draftKey(REPO, CHANGE, 'proposal.md')
    expect(useSpecDraftStore.getState().drafts[key]?.text).toBe('# Change\n')
  })

  it('keys every draft of a change under the exported prefix', () => {
    const s = useSpecDraftStore.getState()
    s.open(REPO, CHANGE, 'proposal.md', 'a')
    s.open(REPO, CHANGE, 'specs/core/spec.md', 'b')
    const prefix = changeDraftPrefix(REPO, CHANGE)
    const keys = Object.keys(useSpecDraftStore.getState().drafts)
    expect(keys).toHaveLength(2)
    expect(keys.every((k) => k.startsWith(prefix))).toBe(true)
  })

  it('builds keys from printable characters only', () => {
    // A control character here is invisible in an editor and in a diff, so a
    // stray one could silently split the store's keys from its readers' again.
    const key = draftKey(REPO, CHANGE, 'proposal.md')
    const hasControlChar = [...key].some((ch) => {
      const code = ch.charCodeAt(0)
      return code < 0x20 || code === 0x7f
    })
    expect(hasControlChar).toBe(false)
    expect(key).toBe(`${REPO} ${CHANGE} proposal.md`)
    expect(changeDraftPrefix(REPO, CHANGE)).toBe(`${REPO} ${CHANGE} `)
  })

  it('holds the file contents it was opened with', () => {
    const s = useSpecDraftStore.getState()
    s.open(REPO, CHANGE, 'proposal.md', '# Change\n')
    expect(useSpecDraftStore.getState().get(REPO, CHANGE, 'proposal.md')?.text).toBe('# Change\n')
  })

  it('keeps unsaved text when the same file is opened again', () => {
    // The read effect can run twice (StrictMode, Fast Refresh, remount). A
    // second open must not wipe what the user has typed since the first.
    const s = useSpecDraftStore.getState()
    s.open(REPO, CHANGE, 'proposal.md', '# On disk\n')
    s.edit(REPO, CHANGE, 'proposal.md', '# Edited\n')
    s.open(REPO, CHANGE, 'proposal.md', '# On disk\n')
    expect(useSpecDraftStore.getState().get(REPO, CHANGE, 'proposal.md')?.text).toBe('# Edited\n')
  })

  it('is not dirty until the text differs from disk', () => {
    const s = useSpecDraftStore.getState()
    s.open(REPO, CHANGE, 'proposal.md', '# Change\n')
    expect(useSpecDraftStore.getState().isDirty(REPO, CHANGE, 'proposal.md')).toBe(false)
    s.edit(REPO, CHANGE, 'proposal.md', '# Change edited\n')
    expect(useSpecDraftStore.getState().isDirty(REPO, CHANGE, 'proposal.md')).toBe(true)
  })

  it('reports dirtiness for the change as a whole', () => {
    const s = useSpecDraftStore.getState()
    s.open(REPO, CHANGE, 'proposal.md', 'a')
    expect(useSpecDraftStore.getState().changeHasDirty(REPO, CHANGE)).toBe(false)
    s.edit(REPO, CHANGE, 'proposal.md', 'b')
    expect(useSpecDraftStore.getState().changeHasDirty(REPO, CHANGE)).toBe(true)
  })

  it('keeps files, changes and repos apart', () => {
    const s = useSpecDraftStore.getState()
    s.open(REPO, CHANGE, 'proposal.md', 'one')
    s.open(REPO, CHANGE, 'tasks.md', 'two')
    s.open(REPO, 'other-change', 'proposal.md', 'three')
    s.open('repo-2', CHANGE, 'proposal.md', 'four')
    const g = useSpecDraftStore.getState()
    expect(g.get(REPO, CHANGE, 'proposal.md')?.text).toBe('one')
    expect(g.get(REPO, CHANGE, 'tasks.md')?.text).toBe('two')
    expect(g.get(REPO, 'other-change', 'proposal.md')?.text).toBe('three')
    expect(g.get('repo-2', CHANGE, 'proposal.md')?.text).toBe('four')
    // Dirt in one change must not mark another.
    g.edit(REPO, 'other-change', 'proposal.md', 'changed')
    expect(useSpecDraftStore.getState().changeHasDirty(REPO, CHANGE)).toBe(false)
    expect(useSpecDraftStore.getState().changeHasDirty(REPO, 'other-change')).toBe(true)
  })

  it('replaces text for a file that was never opened', () => {
    // Accepting an AI draft for a file the user had not opened. It must read as
    // unsaved, or the Save button would be disabled on text nobody has stored.
    const s = useSpecDraftStore.getState()
    s.replace(REPO, CHANGE, 'design.md', '# Design\n')
    const g = useSpecDraftStore.getState()
    expect(g.get(REPO, CHANGE, 'design.md')?.text).toBe('# Design\n')
    expect(g.isDirty(REPO, CHANGE, 'design.md')).toBe(true)
  })

  it('discards a draft entirely', () => {
    const s = useSpecDraftStore.getState()
    s.open(REPO, CHANGE, 'proposal.md', 'a')
    s.edit(REPO, CHANGE, 'proposal.md', 'b')
    s.discard(REPO, CHANGE, 'proposal.md')
    expect(useSpecDraftStore.getState().get(REPO, CHANGE, 'proposal.md')).toBeUndefined()
    expect(useSpecDraftStore.getState().changeHasDirty(REPO, CHANGE)).toBe(false)
  })

  it('opens an empty file without erroring', () => {
    // design.md often does not exist yet; the read returns "" and the editor
    // should hold an empty, not-dirty draft rather than nothing at all.
    const s = useSpecDraftStore.getState()
    s.open(REPO, CHANGE, 'design.md', '')
    const g = useSpecDraftStore.getState()
    expect(g.get(REPO, CHANGE, 'design.md')).toEqual({ text: '', original: '' })
    expect(g.isDirty(REPO, CHANGE, 'design.md')).toBe(false)
  })
})
