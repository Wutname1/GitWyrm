import { useBranches, useStatus } from '@/hooks/useGitQueries'
import { useActiveRepo } from '@/stores/workspaceStore'
import { ChangesList } from './commit-form/ChangesList'
import { CommitMessageForm } from './commit-form/CommitMessageForm'

export function RightPanel() {
  const repo = useActiveRepo()
  const status = useStatus(repo?.id ?? null)
  const branches = useBranches(repo?.id ?? null)

  const currentBranch = branches.data?.local.find((b) => b.is_head)?.name ?? ''
  const total = (status.data?.staged.length ?? 0) + (status.data?.unstaged.length ?? 0)

  return (
    <div className="flex w-80 min-h-0 flex-none flex-col border-l border-border bg-panel">
      <div className="flex flex-none items-center gap-2 border-b border-border px-3.5 pb-[9px] pt-[11px]">
        <span className="text-[11px] font-bold tracking-[.05em] text-sub">
          CHANGES{currentBranch ? ` · ${currentBranch}` : ''}
        </span>
        <span className="ml-auto rounded-full bg-primary px-[7px] py-px font-mono text-[9px] font-bold text-primary-foreground">
          {total}
        </span>
      </div>
      <ChangesList />
      <CommitMessageForm />
    </div>
  )
}
