import { toast } from 'sonner'
import type { SectionItem, SidebarSectionData } from '@/lib/types'
import { useBranches, useRemotes, useStashes, useTags } from '@/hooks/useGitQueries'
import { useGitMutations } from '@/hooks/useGitMutations'
import { useUiStore } from '@/stores/uiStore'
import { useActiveRepo } from '@/stores/workspaceStore'
import { SidebarSection } from './SidebarSection'
import { RemotesSection } from './RemotesSection'

export function LeftPanel() {
  const repo = useActiveRepo()
  const selectCommit = useUiStore((s) => s.selectCommit)
  const openMerge = useUiStore((s) => s.openMerge)
  const openModal = useUiStore((s) => s.openModal)
  const m = useGitMutations(repo?.id ?? null)

  const branches = useBranches(repo?.id ?? null)
  const tags = useTags(repo?.id ?? null)
  const remotes = useRemotes(repo?.id ?? null)
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

  // Section headers that get a hover `+` action, keyed by section key.
  const addAction: Partial<Record<string, { run: () => void; label: string }>> = {
    local: { run: () => openModal('newBranch'), label: 'New branch' },
    tags: { run: () => toast('Right-click a commit to tag it'), label: 'New tag' },
  }

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
    if (section.type === 'branch' && item.name !== currentBranch) {
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

  const localSection = sections[0]
  const otherSections = sections.slice(1)

  return (
    <div className="w-60 flex-none overflow-y-auto border-r border-border bg-panel pb-6 pt-1.5">
      <SidebarSection
        key={localSection.key}
        section={localSection}
        currentBranch={currentBranch}
        onItemClick={onItemClick}
        onItemContextMenu={onItemContextMenu}
        onAdd={addAction.local?.run}
        addLabel={addAction.local?.label}
      />

      <RemotesSection remotes={remotes.data ?? []} onManage={() => openModal('remotes')} />

      {otherSections.map((section) => (
        <SidebarSection
          key={section.key}
          section={section}
          currentBranch={currentBranch}
          onItemClick={onItemClick}
          onItemContextMenu={onItemContextMenu}
          onAdd={addAction[section.key]?.run}
          addLabel={addAction[section.key]?.label}
        />
      ))}
    </div>
  )
}
