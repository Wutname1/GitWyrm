import { ScrollArea } from 'gitwyrm-mockup'
import { GitBranch, GitCommitHorizontal } from 'lucide-react'

const BRANCHES = [
  'main',
  'feature/commit-graph',
  'feature/rebase-ui',
  'fix/detached-head-crash',
  'chore/bump-tauri-v2',
  'release/1.4.0',
  'experiment/inline-diff',
  'hotfix/push-auth-token',
  'docs/keyboard-shortcuts',
  'refactor/graph-layout',
  'feature/stash-list',
  'wip/conflict-resolver',
]

const COMMITS = [
  { sha: 'a1c9f42', msg: 'Fix detached HEAD crash on force-push' },
  { sha: 'b7e0d18', msg: 'Add inline diff toggle to commit drawer' },
  { sha: 'c3f5a90', msg: 'Bump Tauri to v2.1 and regenerate bindings' },
  { sha: 'd9021ab', msg: 'Render stash rows in the graph timeline' },
  { sha: 'e4477cd', msg: 'Debounce branch filter input' },
  { sha: 'f8813ee', msg: 'Cache remote ahead/behind counts' },
  { sha: '0a6b2f1', msg: 'Handle empty repository placeholder state' },
  { sha: '15d9c04', msg: 'Wire up rename branch dialog' },
  { sha: '22ee7b9', msg: 'Add resize handle to left panel' },
  { sha: '3b0f6a2', msg: 'Style conflict banner for dark theme' },
]

export function BranchList() {
  return (
    <div style={{ padding: 24 }}>
      <ScrollArea className="h-56 w-64 rounded-md border border-border">
        <div className="p-2">
          {BRANCHES.map((b) => (
            <div
              key={b}
              className="flex items-center gap-2 rounded px-2 py-1.5 text-sm text-foreground hover:bg-secondary"
            >
              <GitBranch className="size-3.5 text-muted-foreground" />
              {b}
            </div>
          ))}
        </div>
      </ScrollArea>
    </div>
  )
}

export function CommitList() {
  return (
    <div style={{ padding: 24 }}>
      <ScrollArea className="h-56 w-80 rounded-md border border-border">
        <div className="p-2">
          {COMMITS.map((c) => (
            <div key={c.sha} className="flex items-center gap-2 rounded px-2 py-1.5 text-sm">
              <GitCommitHorizontal className="size-3.5 flex-none text-muted-foreground" />
              <span className="font-mono text-xs text-muted-foreground">{c.sha}</span>
              <span className="truncate text-foreground">{c.msg}</span>
            </div>
          ))}
        </div>
      </ScrollArea>
    </div>
  )
}
