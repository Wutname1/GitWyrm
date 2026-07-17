import { useEffect, useRef, useState } from 'react'
import { cn } from '@/lib/utils'
import { useBranches, useStatus } from '@/hooks/useGitQueries'
import { useUiStore } from '@/stores/uiStore'
import { useActiveRepo } from '@/stores/workspaceStore'
import { ChangesList } from './commit-form/ChangesList'
import { ChangesMenu } from './commit-form/ChangesMenu'
import { CommitMessageForm } from './commit-form/CommitMessageForm'

export function RightPanel() {
  const repo = useActiveRepo()
  const status = useStatus(repo?.id ?? null)
  const branches = useBranches(repo?.id ?? null)
  const changesFocusNonce = useUiStore((s) => s.changesFocusNonce)

  const currentBranch = branches.data?.local.find((b) => b.is_head)?.name ?? ''
  const total = (status.data?.staged.length ?? 0) + (status.data?.unstaged.length ?? 0)

  const [flash, setFlash] = useState(false)
  const headerRef = useRef<HTMLDivElement>(null)

  useEffect(() => {
    if (changesFocusNonce === 0) return
    headerRef.current?.scrollIntoView({ block: 'nearest' })
    setFlash(false)
    const raf = requestAnimationFrame(() => setFlash(true))
    const timer = setTimeout(() => setFlash(false), 900)
    return () => {
      cancelAnimationFrame(raf)
      clearTimeout(timer)
    }
  }, [changesFocusNonce])

  return (
    <div
      className={cn(
        'flex w-80 min-h-0 flex-none flex-col border-l bg-panel transition-colors duration-500',
        flash ? 'border-primary' : 'border-border'
      )}
    >
      <ChangesMenu>
        <div
          ref={headerRef}
          className={cn(
            'flex flex-none items-center gap-2 border-b border-border px-3.5 pb-[9px] pt-[11px] transition-colors duration-500',
            flash && 'bg-soft'
          )}
        >
          <span className="text-[11px] font-bold tracking-[.05em] text-sub">
            CHANGES{currentBranch ? ` · ${currentBranch}` : ''}
          </span>
          <span className="ml-auto rounded-full bg-primary px-[7px] py-px font-mono text-[9px] font-bold text-primary-foreground">
            {total}
          </span>
        </div>
      </ChangesMenu>
      <ChangesList />
      <CommitMessageForm />
    </div>
  )
}
