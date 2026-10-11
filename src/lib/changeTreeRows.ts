/**
 * The changed-files list as a flat run of rows, for a virtualized list.
 *
 * The list used to render every changed file as a full row - a right-click menu
 * and a tooltip button each - and to re-render all of them on every click,
 * because each click flips the "something is staging" flag every row reads.
 * With about a thousand changed files that was enough to freeze the window
 * while staging a handful of them. Flattening the visible tree into rows lets
 * the list draw only the rows on screen.
 *
 * The folder totals are worked out once per list in `filesByFolder`, instead of
 * scanning every file again for each folder row as it rendered.
 */

interface HasPath {
  path: string
}

export interface ChangeTreeNode<F extends HasPath> {
  name: string
  path: string
  directories: Map<string, ChangeTreeNode<F>>
  files: Array<{ name: string; file: F }>
}

export type ChangeRow<F extends HasPath> =
  | { kind: 'folder'; key: string; node: ChangeTreeNode<F>; depth: number; expanded: boolean }
  | { kind: 'file'; key: string; file: F; name: string; depth: number | undefined }

const normalize = (path: string) => path.replaceAll('\\', '/')

export function buildChangeTree<F extends HasPath>(files: F[]): ChangeTreeNode<F> {
  const root: ChangeTreeNode<F> = { name: '', path: '', directories: new Map(), files: [] }
  for (const file of files) {
    const segments = normalize(file.path).split('/').filter(Boolean)
    const fileName = segments.pop() ?? file.path
    let node = root
    for (const segment of segments) {
      const path = node.path ? `${node.path}/${segment}` : segment
      let child = node.directories.get(segment)
      if (!child) {
        child = { name: segment, path, directories: new Map(), files: [] }
        node.directories.set(segment, child)
      }
      node = child
    }
    node.files.push({ name: fileName, file })
  }
  return root
}

/** Every folder key under `node`, prefixed with `treeId` the way rows are keyed. */
export function folderKeys<F extends HasPath>(node: ChangeTreeNode<F>, treeId: string): string[] {
  const keys: string[] = []
  for (const directory of node.directories.values()) {
    keys.push(`${treeId}:${directory.path}`, ...folderKeys(directory, treeId))
  }
  return keys
}

const byName = (left: { name: string }, right: { name: string }) =>
  left.name.localeCompare(right.name)

/**
 * The rows a person can see: folders, and the contents of the open ones, in
 * the order the tree draws them (folders before files, each sorted by name).
 */
export function flattenVisible<F extends HasPath>(
  root: ChangeTreeNode<F>,
  expanded: ReadonlySet<string>,
  treeId: string,
): ChangeRow<F>[] {
  const rows: ChangeRow<F>[] = []
  const walk = (node: ChangeTreeNode<F>, depth: number) => {
    for (const directory of [...node.directories.values()].sort(byName)) {
      const key = `${treeId}:${directory.path}`
      const isOpen = expanded.has(key)
      rows.push({ kind: 'folder', key, node: directory, depth, expanded: isOpen })
      if (isOpen) walk(directory, depth + 1)
    }
    for (const { name, file } of [...node.files].sort(byName)) {
      rows.push({ kind: 'file', key: `${treeId}:file:${file.path}`, file, name, depth })
    }
  }
  walk(root, 0)
  return rows
}

/** One row per file, full path as the label, sorted by path. */
export function flatRows<F extends HasPath>(files: F[], treeId: string): ChangeRow<F>[] {
  return [...files]
    .sort((left, right) => left.path.localeCompare(right.path))
    .map((file) => ({
      kind: 'file' as const,
      key: `${treeId}:file:${file.path}`,
      file,
      name: file.path,
      depth: undefined,
    }))
}

/**
 * Every file under each folder, keyed by folder path, built in one pass.
 *
 * A file at `a/b/c.txt` is listed under `a` and `a/b`. A path that appears
 * twice (staged and unstaged halves of one file) is listed once.
 */
export function filesByFolder<F extends HasPath>(files: F[]): Map<string, F[]> {
  const seen = new Map<string, Map<string, F>>()
  for (const file of files) {
    const normalized = normalize(file.path)
    const segments = normalized.split('/').filter(Boolean)
    segments.pop()
    let folder = ''
    for (const segment of segments) {
      folder = folder ? `${folder}/${segment}` : segment
      let bucket = seen.get(folder)
      if (!bucket) {
        bucket = new Map()
        seen.set(folder, bucket)
      }
      bucket.set(normalized, file)
    }
  }
  const out = new Map<string, F[]>()
  for (const [folder, bucket] of seen) out.set(folder, [...bucket.values()])
  return out
}
