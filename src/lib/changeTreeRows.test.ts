import { describe, expect, it } from 'vitest'
import {
  buildChangeTree,
  filesByFolder,
  flatRows,
  flattenVisible,
  folderKeys,
} from './changeTreeRows'

const f = (path: string) => ({ path })

describe('changed-file rows', () => {
  const files = [f('src/b.ts'), f('src/a.ts'), f('src/lib/x.ts'), f('README.md')]
  const root = buildChangeTree(files)

  it('shows only top-level folders and files while everything is closed', () => {
    const rows = flattenVisible(root, new Set(), 'u')
    expect(rows.map((r) => (r.kind === 'folder' ? `dir:${r.node.path}` : r.file.path))).toEqual([
      'dir:src',
      'README.md',
    ])
  })

  it('opens folders in tree order, folders before files', () => {
    const rows = flattenVisible(root, new Set(['u:src', 'u:src/lib']), 'u')
    expect(rows.map((r) => (r.kind === 'folder' ? `dir:${r.node.path}` : r.file.path))).toEqual([
      'dir:src',
      'dir:src/lib',
      'src/lib/x.ts',
      'src/a.ts',
      'src/b.ts',
      'README.md',
    ])
    expect(rows.map((r) => r.depth)).toEqual([0, 1, 2, 1, 1, 0])
  })

  it('keys every row uniquely', () => {
    const rows = flattenVisible(root, new Set(folderKeys(root, 'u')), 'u')
    expect(new Set(rows.map((r) => r.key)).size).toBe(rows.length)
  })

  it('lists files flat by full path', () => {
    expect(flatRows(files, 's').map((r) => r.kind === 'file' && r.name)).toEqual([
      'README.md',
      'src/a.ts',
      'src/b.ts',
      'src/lib/x.ts',
    ])
  })

  it('collects every file under each folder once', () => {
    const map = filesByFolder([...files, f('src/a.ts')])
    expect(map.get('src')?.map((x) => x.path).sort()).toEqual(['src/a.ts', 'src/b.ts', 'src/lib/x.ts'])
    expect(map.get('src/lib')?.map((x) => x.path)).toEqual(['src/lib/x.ts'])
    expect(map.has('')).toBe(false)
  })

  // The case that froze the window: a thousand changed files in a few folders.
  // Building the rows must stay cheap, and a closed folder must cost one row.
  it('handles a thousand files without drawing the closed ones', () => {
    const many = Array.from({ length: 1000 }, (_, i) => f(`pkg${i % 10}/sub/file${i}.ts`))
    const started = performance.now()
    const tree = buildChangeTree(many)
    const closed = flattenVisible(tree, new Set(), 'u')
    const byFolder = filesByFolder(many)
    expect(performance.now() - started).toBeLessThan(500)
    expect(closed).toHaveLength(10)
    expect(byFolder.get('pkg0')).toHaveLength(100)
  })
})
