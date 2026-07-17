import { Button } from '@/components/ui/button'
import { useStatus } from '@/hooks/useGitQueries'
import { useGitMutations } from '@/hooks/useGitMutations'
import { useUiStore } from '@/stores/uiStore'
import { useActiveRepo } from '@/stores/workspaceStore'
import { FileChangeRow, StageToggle } from '../FileChangeRow'

function GroupHeader({ label, count, action, onAction }: { label: string; count: number; action: string; onAction: () => void }) {
  return (
    <div className="sticky top-0 z-[2] flex items-center gap-2 bg-panel px-3.5 py-[7px]">
      <span className="text-[10px] font-bold tracking-[.05em] text-sub">{label}</span>
      <span className="font-mono text-[9.5px] text-muted-foreground">{count}</span>
      <Button
        variant="secondary"
        size="sm"
        onClick={onAction}
        className="ml-auto h-auto rounded border-border bg-panel3 px-[7px] py-0.5 text-[10px] text-sub"
      >
        {action}
      </Button>
    </div>
  )
}

export function ChangesList() {
  const repo = useActiveRepo()
  const status = useStatus(repo?.id ?? null)
  const openDiff = useUiStore((s) => s.openDiff)
  const m = useGitMutations(repo?.id ?? null)

  const staged = status.data?.staged ?? []
  const unstaged = status.data?.unstaged ?? []

  return (
    <div className="min-h-0 flex-1 overflow-y-auto">
      <GroupHeader
        label="STAGED"
        count={staged.length}
        action="Unstage all"
        onAction={() => m.unstageAll.mutate()}
      />
      {staged.map((f) => (
        <FileChangeRow
          key={`s-${f.path}`}
          file={f}
          onOpen={() => openDiff({ path: f.path, source: { kind: 'staged' } })}
          action={
            <StageToggle
              direction="unstage"
              onToggle={(e) => {
                e.stopPropagation()
                m.unstageFile.mutate(f.path)
              }}
            />
          }
        />
      ))}

      <div className="mt-1">
        <GroupHeader
          label="UNSTAGED"
          count={unstaged.length}
          action="Stage all"
          onAction={() => m.stageAll.mutate()}
        />
      </div>
      {unstaged.map((f) => (
        <FileChangeRow
          key={`u-${f.path}`}
          file={f}
          nameClassName={f.conflicted ? 'text-removed' : 'text-foreground'}
          onOpen={() => openDiff({ path: f.path, source: { kind: 'unstaged' } })}
          action={
            <StageToggle
              direction="stage"
              onToggle={(e) => {
                e.stopPropagation()
                m.stageFile.mutate(f.path)
              }}
            />
          }
        />
      ))}
      {status.data && staged.length === 0 && unstaged.length === 0 && (
        <div className="p-4 text-center text-[11px] text-muted-foreground">Working tree clean</div>
      )}
    </div>
  )
}
