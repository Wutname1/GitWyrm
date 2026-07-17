import { useBranches, useStatus } from '@/hooks/useGitQueries'
import { useActiveRepo } from '@/stores/workspaceStore'

export function StatusBar() {
  const repo = useActiveRepo()
  const status = useStatus(repo?.id ?? null)
  const branches = useBranches(repo?.id ?? null)

  const head = branches.data?.local.find((b) => b.is_head)
  const total = (status.data?.staged.length ?? 0) + (status.data?.unstaged.length ?? 0)

  return (
    <div className="flex h-6 flex-none items-center gap-4 border-t border-border bg-panel2 px-3 font-mono text-[10.5px] text-sub">
      {head && (
        <span className="flex items-center gap-[5px]">
          <span className="size-[7px] rounded-[2px] bg-primary" />
          {head.name}
        </span>
      )}
      {(head?.ahead || head?.behind) ? (
        <span>
          {head.ahead ? `↑${head.ahead}` : ''}
          {head.ahead && head.behind ? ' ' : ''}
          {head.behind ? `↓${head.behind}` : ''}
        </span>
      ) : null}
      <span className="text-muted-foreground">{total} changes</span>
      <div className="flex-1" />
      {repo && <span className="text-muted-foreground">{repo.path}</span>}
      {head?.upstream && (
        <span className="flex items-center gap-[5px] text-muted-foreground">
          <span className="size-1.5 rounded-full bg-added" />
          {head.upstream}
        </span>
      )}
    </div>
  )
}
