import { toast } from 'sonner'
import type { SectionItem, SidebarSectionData } from '@/lib/types'
import { useBranches, useStashes, useTags } from '@/hooks/useGitQueries'
import { useGitMutations } from '@/hooks/useGitMutations'
import { useUiStore } from '@/stores/uiStore'
import { useActiveRepo } from '@/stores/workspaceStore'
import { SidebarSection } from './SidebarSection'

export function LeftPanel() {
  const repo = useActiveRepo()
  const selectCommit = useUiStore((s) => s.selectCommit)
  const openMerge = useUiStore((s) => s.openMerge)
  const m = useGitMutations(repo?.id ?? null)

  const branches = useBranches(repo?.id ?? null)
  const tags = useTags(repo?.id ?? null)
  const stashes = useStashes(repo?.id ?? null)

  const currentBranch =
    branches.data?.local.find((b) => b.is_head)?.name ?? repo?.head_branch ?? ''

  const sections: SidebarSectionData[] = [
    {
      key: 'local',
      label: 'LOCAL',
      type: 'branch',
      items: (branches.data?.local ?? []).map((b) => ({
        name: b.name,
        meta:
          b.ahead || b.behind
            ? `${b.ahead ? `↑${b.ahead}` : ''}${b.ahead && b.behind ? ' ' : ''}${b.behind ? `↓${b.behind}` : ''}`
            : undefined,
      })),
    },
    {
      key: 'remote',
      label: 'REMOTE',
      type: 'remote',
      items: (branches.data?.remote ?? []).map((name) => ({ name })),
    },
    {
      key: 'stashes',
      label: 'STASHES',
      type: 'stash',
      items: (stashes.data ?? []).map((s) => ({ name: s.message })),
    },
    {
      key: 'prs',
      label: 'PULL REQUESTS',
      type: 'pr',
      items: [{ name: 'Connect GitHub (soon)' }],
    },
    {
      key: 'issues',
      label: 'ISSUES',
      type: 'issue',
      items: [{ name: 'Connect GitHub (soon)' }],
    },
    {
      key: 'tags',
      label: 'TAGS',
      type: 'tag',
      items: (tags.data ?? []).map((t) => ({ name: t.name })),
    },
  ]

  const onItemClick = (section: SidebarSectionData, item: SectionItem) => {
    switch (section.type) {
      case 'branch':
        if (item.name === currentBranch) return
        selectCommit(null)
        m.checkout.mutate(item.name)
        break
      case 'stash': {
        const idx = (stashes.data ?? []).findIndex((s) => s.message === item.name)
        if (idx >= 0) m.stashPop.mutate(idx)
        break
      }
      case 'pr':
      case 'issue':
        toast('GitHub integration is planned')
        break
      default:
        toast(item.name)
    }
  }

  const onItemContextMenu = (
    section: SidebarSectionData,
    item: SectionItem,
    e: React.MouseEvent
  ) => {
    if ((section.type === 'branch' || section.type === 'remote') && item.name !== currentBranch) {
      e.preventDefault()
      openMerge(item.name)
    }
  }

  if (!repo) {
    return (
      <div className="w-60 flex-none border-r border-border bg-panel p-4 text-xs text-muted-foreground">
        No repository open
      </div>
    )
  }

  return (
    <div className="w-60 flex-none overflow-y-auto border-r border-border bg-panel pb-6 pt-1.5">
      {sections.map((section) => (
        <SidebarSection
          key={section.key}
          section={section}
          currentBranch={currentBranch}
          onItemClick={onItemClick}
          onItemContextMenu={onItemContextMenu}
        />
      ))}
    </div>
  )
}
