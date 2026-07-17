// Slash-delimited branch names (e.g. dependabot/npm/foo) fold into nested
// folders. A leaf carries the full branch name; a folder has children only.

export interface BranchTreeNode {
  name: string
  /** Full branch name when this node is a leaf (a real branch), else null. */
  branch: string | null
  children: BranchTreeNode[]
}

export function buildBranchTree(branches: string[]): BranchTreeNode[] {
  const root: BranchTreeNode = { name: '', branch: null, children: [] }
  for (const full of branches) {
    const parts = full.split('/')
    let node = root
    parts.forEach((part, i) => {
      const isLeaf = i === parts.length - 1
      let child = node.children.find((c) => c.name === part && (isLeaf ? !!c.branch : !c.branch))
      if (!child) {
        child = { name: part, branch: isLeaf ? full : null, children: [] }
        node.children.push(child)
      }
      node = child
    })
  }
  const sort = (nodes: BranchTreeNode[]) => {
    // Folders first, then leaves; alphabetical within each group.
    nodes.sort((a, b) => {
      const fa = a.branch ? 1 : 0
      const fb = b.branch ? 1 : 0
      return fa - fb || a.name.localeCompare(b.name)
    })
    nodes.forEach((n) => sort(n.children))
  }
  sort(root.children)
  return root.children
}
