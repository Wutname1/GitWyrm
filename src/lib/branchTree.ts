// Slash-delimited branch names (e.g. dependabot/npm/foo) fold into nested
// folders. A leaf carries the full branch name; a folder has children only.

export interface BranchTreeNode<T = never> {
  name: string
  /** Full branch name when this node is a leaf (a real branch), else null. */
  branch: string | null
  /** Payload for leaves built from richer records; null for folders. */
  data: T | null
  children: BranchTreeNode<T>[]
}

function insert<T>(root: BranchTreeNode<T>, full: string, data: T | null) {
  const parts = full.split('/')
  let node = root
  parts.forEach((part, i) => {
    const isLeaf = i === parts.length - 1
    let child = node.children.find((c) => c.name === part && (isLeaf ? !!c.branch : !c.branch))
    if (!child) {
      child = { name: part, branch: isLeaf ? full : null, data: isLeaf ? data : null, children: [] }
      node.children.push(child)
    }
    node = child
  })
}

function sortTree<T>(nodes: BranchTreeNode<T>[]) {
  // Folders first, then leaves; alphabetical within each group.
  nodes.sort((a, b) => {
    const fa = a.branch ? 1 : 0
    const fb = b.branch ? 1 : 0
    return fa - fb || a.name.localeCompare(b.name)
  })
  nodes.forEach((n) => sortTree(n.children))
}

/** Fold a list into the folder tree; leaves keep the record they came from. */
export function buildBranchTreeFrom<T>(items: T[], nameOf: (item: T) => string): BranchTreeNode<T>[] {
  const root: BranchTreeNode<T> = { name: '', branch: null, data: null, children: [] }
  for (const item of items) insert(root, nameOf(item), item)
  sortTree(root.children)
  return root.children
}
