import type { ReactNode } from 'react'
import { Button } from '@/components/ui/button'
import { PendingIndicator } from '@/components/ui/pending-indicator'
import { cn } from '@/lib/utils'
import { useStatus } from '@/hooks/useGitQueries'
import { useGitMutations } from '@/hooks/useGitMutations'
import { useUiStore } from '@/stores/uiStore'
import { useActiveRepo } from '@/stores/workspaceStore'
import { FileChangeRow, StageToggle } from '../FileChangeRow'

function GroupHeader({
  label,
  count,
  tone,
  children,
}: {
  label: string
  count: number
  tone: 'staged' | 'unstaged'
  children: ReactNode
}) {
  return (
    <div className="sticky top-0 z-[2] flex items-center gap-2 bg-panel px-3.5 py-[7px]">
      <span
        className={cn(
          'size-1.5 flex-none rounded-full',
          tone === 'staged' ? 'bg-primary' : 'bg-modified'
        )}
      />
      <span className="text-2xs font-bold tracking-[.05em] text-sub">{label}</span>
      <span className="font-mono text-2xs text-muted-foreground">{count}</span>
      <div className="ml-auto flex items-center">{children}</div>
    </div>
  )
}

export function ChangesList() {
  const repo = useActiveRepo()
  const status = useStatus(repo?.id ?? null)
  const openDiff = useUiStore((s) => s.openDiff)
  const openConflict = useUiStore((s) => s.openConflict)
  const m = useGitMutations(repo?.id ?? null)

  const staged = status.data?.staged ?? []
  const unstaged = status.data?.unstaged ?? []
  const hasChanges = staged.length > 0 || unstaged.length > 0
  const stagingPending =
    m.stageFile.isPending ||
    m.unstageFile.isPending ||
    m.stageAll.isPending ||
    m.unstageAll.isPending

  return (
    <div className="min-h-0 flex-1 overflow-y-auto">
      {hasChanges && (
        <div className="flex items-center gap-2 border-b border-dashed border-primary/40 bg-soft px-3.5 py-[6px]">
          <span className="relative flex size-2 flex-none items-center justify-center">
            <span className="absolute inline-flex size-2 animate-ping rounded-full bg-primary/60" />
            <span className="relative inline-flex size-1.5 rounded-full bg-primary" />
          </span>
          <span className="text-2xs font-semibold tracking-[.04em] text-accent-text">
            Pending changes
          </span>
          <span className="ml-auto font-mono text-2xs text-sub">
            {staged.length + unstaged.length} file{staged.length + unstaged.length === 1 ? '' : 's'}
          </span>
        </div>
      )}

      <GroupHeader label="STAGED" count={staged.length} tone="staged">
        {staged.length > 0 && (
          <Button
            variant="ghost"
            size="sm"
            onClick={() => m.unstageAll.mutate()}
            disabled={stagingPending}
            className="h-auto rounded px-[7px] py-0.5 text-2xs text-sub hover:bg-panel3 hover:text-foreground"
          >
            {m.unstageAll.isPending && <PendingIndicator className="size-3" />}
            {m.unstageAll.isPending ? 'Unstaging…' : 'Unstage all'}
          </Button>
        )}
      </GroupHeader>
      {staged.map((f) => (
        <FileChangeRow
          key={`s-${f.path}`}
          file={f}
          menuStaged
          onOpen={() => openDiff({ path: f.path, source: { kind: 'staged' } })}
          action={
            <StageToggle
              direction="unstage"
              disabled={stagingPending}
              pending={m.unstageFile.isPending && m.unstageFile.variables === f.path}
              onToggle={(e) => {
                e.stopPropagation()
                m.unstageFile.mutate(f.path)
              }}
            />
          }
        />
      ))}
      {staged.length === 0 && hasChanges && (
        <div className="px-3.5 py-1.5 text-2xs italic text-muted-foreground">
          Nothing staged yet
        </div>
      )}

      <div className="my-1 border-t-2 border-border/70" />

      <GroupHeader label="UNSTAGED" count={unstaged.length} tone="unstaged">
        {unstaged.length > 0 && (
          <Button
            size="sm"
            onClick={() => m.stageAll.mutate()}
            disabled={stagingPending}
            className="h-auto rounded border border-primary/50 bg-soft px-2 py-0.5 text-2xs font-semibold text-accent-text hover:border-primary hover:bg-primary hover:text-primary-foreground"
          >
            {m.stageAll.isPending && <PendingIndicator className="size-3" />}
            {m.stageAll.isPending ? 'Staging…' : 'Stage all'}
          </Button>
        )}
      </GroupHeader>
      {unstaged.map((f) =>
        f.conflicted ? (
          <FileChangeRow
            key={`u-${f.path}`}
            file={f}
            menuStaged={false}
            nameClassName="text-removed"
            onOpen={() => openConflict(f.path)}
            action={
              <button
                onClick={(e) => {
                  e.stopPropagation()
                  openConflict(f.path)
                }}
                className="flex-none rounded border border-removed/50 bg-removed/10 px-1.5 py-0.5 text-2xs font-semibold text-removed hover:bg-removed/20"
              >
                Resolve
              </button>
            }
          />
        ) : (
          <FileChangeRow
            key={`u-${f.path}`}
            file={f}
            menuStaged={false}
            onOpen={() => openDiff({ path: f.path, source: { kind: 'unstaged' } })}
            action={
              <StageToggle
                direction="stage"
                disabled={stagingPending}
                pending={m.stageFile.isPending && m.stageFile.variables === f.path}
                onToggle={(e) => {
                  e.stopPropagation()
                  m.stageFile.mutate(f.path)
                }}
              />
            }
          />
        )
      )}
      {status.data && !hasChanges && (
        <div className="p-4 text-center text-2xs text-muted-foreground">Working tree clean</div>
      )}
    </div>
  )
}
