import { describe, it, expect } from 'vitest'
import {
  addFolder,
  deserializeCodeFolders,
  primaryFirst,
  primaryPathOf,
  removeFolder,
  setFolderLabel,
  setPrimaryFolder,
  type CodeFolder,
} from './codeFolders'

const paths = (folders: CodeFolder[]) => folders.map((f) => f.path)

describe('deserializeCodeFolders', () => {
  /**
   * The upgrade path. Someone who set a single watched folder before the list
   * existed must find it still watched -- and starred -- after updating,
   * without touching anything.
   */
  it('migrates a pre-list code_folder into a single primary folder', () => {
    const folders = deserializeCodeFolders(undefined, 'C:\\code')
    expect(paths(folders)).toEqual(['C:\\code'])
    expect(folders[0].primary).toBe(true)
  })

  it('ignores the legacy folder once a list exists', () => {
    const folders = deserializeCodeFolders([{ path: 'D:\\code', primary: true }], 'C:\\code')
    expect(paths(folders)).toEqual(['D:\\code'])
  })

  it('returns nothing when neither the list nor the legacy folder is set', () => {
    expect(deserializeCodeFolders(undefined, null)).toEqual([])
    expect(deserializeCodeFolders([], '')).toEqual([])
    expect(deserializeCodeFolders([], '   ')).toEqual([])
  })

  it('drops blank paths and duplicates that differ only by slash or case', () => {
    const folders = deserializeCodeFolders(
      [
        { path: 'C:\\code', primary: true },
        { path: 'C:/code/' },
        { path: 'c:\\CODE' },
        { path: '   ' },
        { path: 'D:\\code' },
      ],
      null,
    )
    expect(paths(folders)).toEqual(['C:\\code', 'D:\\code'])
  })

  /**
   * A non-empty list must never be without a primary: clone and new-repo read
   * it for their default destination, so losing it silently sends new work to
   * the wrong place.
   */
  it('promotes the first folder when the stored list has no primary', () => {
    const folders = deserializeCodeFolders([{ path: 'C:\\code' }, { path: 'D:\\code' }], null)
    expect(primaryPathOf(folders)).toBe('C:\\code')
  })

  it('keeps only the first primary when several are flagged', () => {
    const folders = deserializeCodeFolders(
      [
        { path: 'C:\\code', primary: true },
        { path: 'D:\\code', primary: true },
      ],
      null,
    )
    expect(primaryPathOf(folders)).toBe('C:\\code')
    expect(folders.filter((f) => f.primary)).toHaveLength(1)
  })

  it('keeps a blank label as null rather than an empty string', () => {
    const folders = deserializeCodeFolders([{ path: 'C:\\code', label: '  ' }], null)
    expect(folders[0].label).toBeNull()
  })
})

describe('primaryFirst', () => {
  it('sorts the starred folder first and leaves the rest in order', () => {
    const folders = deserializeCodeFolders(
      [{ path: 'A:\\a' }, { path: 'B:\\b' }, { path: 'C:\\c', primary: true }],
      null,
    )
    expect(paths(primaryFirst(folders))).toEqual(['C:\\c', 'A:\\a', 'B:\\b'])
  })

  it('leaves an empty list alone', () => {
    expect(primaryFirst([])).toEqual([])
  })
})

describe('addFolder', () => {
  it('makes the very first folder primary', () => {
    const folders = addFolder([], 'C:\\code')
    expect(primaryPathOf(folders)).toBe('C:\\code')
  })

  it('does not steal primary from the existing folder', () => {
    const folders = addFolder(addFolder([], 'C:\\code'), 'D:\\code')
    expect(primaryPathOf(folders)).toBe('C:\\code')
    expect(folders).toHaveLength(2)
  })

  it('ignores a folder already watched, whatever the slash style or case', () => {
    const one = addFolder([], 'C:\\code')
    expect(addFolder(one, 'C:/code')).toHaveLength(1)
    expect(addFolder(one, 'c:\\CODE')).toHaveLength(1)
  })

  it('ignores a blank path', () => {
    expect(addFolder([], '   ')).toEqual([])
  })
})

describe('removeFolder', () => {
  /** Removing the starred folder must leave a primary behind, not a list without one. */
  it('promotes the next folder when the primary is removed', () => {
    const folders = addFolder(addFolder([], 'C:\\code'), 'D:\\code')
    const after = removeFolder(folders, 'C:\\code')
    expect(paths(after)).toEqual(['D:\\code'])
    expect(primaryPathOf(after)).toBe('D:\\code')
  })

  it('leaves primary alone when a non-primary folder is removed', () => {
    const folders = addFolder(addFolder([], 'C:\\code'), 'D:\\code')
    expect(primaryPathOf(removeFolder(folders, 'D:\\code'))).toBe('C:\\code')
  })

  it('returns the list unchanged for a folder that is not watched', () => {
    const folders = addFolder([], 'C:\\code')
    expect(removeFolder(folders, 'Z:\\nope')).toBe(folders)
  })
})

describe('setPrimaryFolder', () => {
  it('moves the star and leaves exactly one', () => {
    const folders = addFolder(addFolder([], 'C:\\code'), 'D:\\code')
    const after = setPrimaryFolder(folders, 'D:\\code')
    expect(primaryPathOf(after)).toBe('D:\\code')
    expect(after.filter((f) => f.primary)).toHaveLength(1)
  })

  it('changes nothing for a folder that is not watched', () => {
    const folders = addFolder([], 'C:\\code')
    expect(setPrimaryFolder(folders, 'Z:\\nope')).toBe(folders)
  })
})

describe('setFolderLabel', () => {
  it('sets a label and clears it back to null when blanked', () => {
    const folders = addFolder([], 'C:\\code')
    const named = setFolderLabel(folders, 'C:\\code', 'Main work')
    expect(named[0].label).toBe('Main work')
    expect(setFolderLabel(named, 'C:\\code', '   ')[0].label).toBeNull()
  })
})
