import { type ReactNode, useState } from 'react'
import { ArrowLeftRight, Tag, Trash2 } from 'lucide-react'
import { toast } from 'sonner'
import type { SectionItem, SidebarSectionData } from '@/lib/types'
import { useBranches, useRemotes, useStashes, useTags } from '@/hooks/useGitQueries'
import { useGitMutations } from '@/hooks/useGitMutations'
import { useGithubAuth, useGithubIssues, useGithubPrs, useGithubSlug } from '@/hooks/useGithub'
import { useUiStore } from '@/stores/uiStore'
import { useActiveRepo } from '@/stores/workspaceStore'
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuSeparator,
  ContextMenuTrigger,
} from '@/components/ui/context-menu'
import { ConfirmDialog } from '@/components/modals/ConfirmDialog'
import { RenameBranchDialog } from '@/components/modals/RenameBranchDialog'
import { branchSync } from '@/lib/branchActions'
import { BranchMenu } from '@/components/domain/branch/BranchMenu'
import { SidebarSection } from './SidebarSection'
import { RemotesSection } from './RemotesSection'

export function LeftPanel() {
  const repo = useActiveRepo()
  const selectCommit = useUiStore((s) => s.selectCommit)
  const revealRefInGraph = useUiStore((s) => s.revealRefInGraph)
  const openMerge = useUiStore((s) => s.openMerge)
  const openNewTag = useUiStore((s) => s.openNewTag)
  const openModal = useUiStore((s) => s.openModal)
  const m = useGitMutations(repo?.id ?? null)

  const branches = useBranches(repo?.id ?? null)
  const tags = useTags(repo?.id ?? null)
  const remotes = useRemotes(repo?.id ?? null)
  const stashes = useStashes(repo?.id ?? null)

  const githubSlug = useGithubSlug(repo?.id ?? null)
  const githubAuth = useGithubAuth()
  const githubConnected = githubAuth.data != null
  const prs = useGithubPrs(githubSlug.data, githubConnected)
  const issues = useGithubIssues(githubSlug.data, githubConnected)
  const openGithubItem = useUiStore((s) => s.openGithubItem)

  const [toDelete, setToDelete] = useState<{ kind: 'branch' | 'tag'; name: string } | null>(null)
  const branchToRename = useUiStore((s) => s.branchToRename)
  const branchToDelete = useUiStore((s) => s.branchToDelete)
  const renameBranchPrompt = useUiStore((s) => s.renameBranchPrompt)
  const deleteBranchPrompt = useUiStore((s) => s.deleteBranchPrompt)

  const currentBranch =
    branches.data?.local.find((b) => b.is_head)?.name ?? repo?.head_branch ?? ''

  const sections: SidebarSectionData[] = [
    {
      key: 'local',
      label: 'LOCAL',
      type: 'branch',
      items: (branches.data?.local ?? []).map((b) => {
        const sync = branchSync(b)
        return { name: b.name, meta: sync.text ?? undefined, metaTitle: sync.title ?? undefined }
      }),
    },
    {
      key: 'stashes',
      label: 'STASHES',
      type: 'stash',
      items: (stashes.data ?? []).map((s) => ({ name: s.message })),
    },
    // PR and issue sections only exist for repos hosted on github.com.
    ...(githubSlug.data == null
      ? []
      : ([
          {
            key: 'prs',
            label: 'PULL REQUESTS',
            type: 'pr',
            items: githubConnected
              ? (prs.data ?? []).map((p) => ({
                  name: p.title,
                  meta: `#${p.number}`,
                  metaTitle: `#${p.number} by ${p.author}${p.draft ? ' · draft' : ''}`,
                  id: p.number,
                }))
              : [{ name: 'Connect GitHub' }],
          },
          {
            key: 'issues',
            label: 'ISSUES',
            type: 'issue',
            items: githubConnected
              ? (issues.data ?? []).map((i) => ({
                  name: i.title,
                  meta: `#${i.number}`,
                  metaTitle: `#${i.number} by ${i.author}`,
                  id: i.number,
                }))
              : [{ name: 'Connect GitHub' }],
          },
        ] satisfies SidebarSectionData[])),
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
    tags: { run: () => openNewTag(), label: 'New tag' },
  }

  const isItemPending = (section: SidebarSectionData, item: SectionItem) =>
    (section.type === 'branch' && m.checkout.isPending && m.checkout.variables === item.name) ||
    (section.type === 'stash' &&
      m.stashPop.isPending &&
      (stashes.data ?? []).findIndex((s) => s.message === item.name) === m.stashPop.variables)

  const isItemDisabled = (section: SidebarSectionData, item: SectionItem) =>
    isItemPending(section, item) ||
    (section.type === 'branch' && m.checkout.isPending) ||
    (section.type === 'stash' && m.stashPop.isPending)

  const getPendingLabel = (section: SidebarSectionData, item: SectionItem) =>
    section.type === 'branch' ? `Switching to ${item.name}…` : 'Restoring stash…'

  // Switch to a branch. Guards against re-checking out the current branch and
  // against firing mid-checkout.
  const switchToBranch = (name: string) => {
    if (name === currentBranch || m.checkout.isPending) return
    selectCommit(null)
    m.checkout.mutate(name)
  }

  // Single click reveals a branch in the graph (scroll to and highlight its
  // tip); double click or the hover swap icon switches to it.
  const onItemClick = (section: SidebarSectionData, item: SectionItem) => {
    switch (section.type) {
      case 'branch':
        revealRefInGraph(item.name)
        break
      case 'stash': {
        const idx = (stashes.data ?? []).findIndex((s) => s.message === item.name)
        if (idx >= 0) m.stashPop.mutate(idx)
        break
      }
      case 'pr':
      case 'issue':
        if (item.id == null) openModal('githubConnect')
        else openGithubItem(section.type === 'pr' ? 'pr' : 'issue', item.id)
        break
      default:
        toast(item.name)
    }
  }

  const onItemDoubleClick = (section: SidebarSectionData, item: SectionItem) => {
    if (section.type === 'branch') switchToBranch(item.name)
  }

  // A quick-switch icon appears on hover for branches other than the current one.
  const getHoverAction = (section: SidebarSectionData, item: SectionItem) =>
    section.type === 'branch' && item.name !== currentBranch
      ? {
          icon: <ArrowLeftRight size={12} strokeWidth={2.2} />,
          title: `Switch to ${item.name}`,
          onClick: () => switchToBranch(item.name),
        }
      : undefined

  // Right-click menus for branch and tag rows. Other section types have none.
  const renderItemMenu = (
    section: SidebarSectionData,
    item: SectionItem,
    row: ReactNode
  ): ReactNode => {
    if (section.type === 'branch') {
      return (
        <ContextMenu>
          <ContextMenuTrigger asChild>{row}</ContextMenuTrigger>
          <ContextMenuContent className="w-60">
            <BranchMenu branch={item.name} />
          </ContextMenuContent>
        </ContextMenu>
      )
    }
    if (section.type === 'tag') {
      return (
        <ContextMenu>
          <ContextMenuTrigger asChild>{row}</ContextMenuTrigger>
          <ContextMenuContent className="w-52">
            <ContextMenuItem onSelect={() => openNewTag()}>
              <Tag />
              New tag
            </ContextMenuItem>
            <ContextMenuSeparator />
            <ContextMenuItem
              variant="destructive"
              onSelect={() => setToDelete({ kind: 'tag', name: item.name })}
            >
              <Trash2 />
              Delete tag
            </ContextMenuItem>
          </ContextMenuContent>
        </ContextMenu>
      )
    }
    return null
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
    <div data-dim-on-drag className="w-60 flex-none overflow-y-auto border-r border-border bg-panel pb-6 pt-1.5">
      <SidebarSection
        key={localSection.key}
        section={localSection}
        currentBranch={currentBranch}
        onItemClick={onItemClick}
        onItemDoubleClick={onItemDoubleClick}
        renderItemMenu={renderItemMenu}
        onAdd={addAction.local?.run}
        addLabel={addAction.local?.label}
        isItemPending={isItemPending}
        isItemDisabled={isItemDisabled}
        getPendingLabel={getPendingLabel}
        getHoverAction={getHoverAction}
      />

      <RemotesSection remotes={remotes.data ?? []} onManage={() => openModal('remotes')} />

      {otherSections.map((section) => (
        <SidebarSection
          key={section.key}
          section={section}
          currentBranch={currentBranch}
          onItemClick={onItemClick}
          onItemDoubleClick={onItemDoubleClick}
          renderItemMenu={renderItemMenu}
          onAdd={addAction[section.key]?.run}
          addLabel={addAction[section.key]?.label}
          isItemPending={isItemPending}
          isItemDisabled={isItemDisabled}
          getPendingLabel={getPendingLabel}
          getHoverAction={getHoverAction}
        />
      ))}

      <ConfirmDialog
        open={branchToDelete !== null}
        onOpenChange={(o) => !o && deleteBranchPrompt(null)}
        destructive
        title="Delete this branch?"
        description={
          <>
            This deletes the local branch{' '}
            <span className="font-mono text-foreground">{branchToDelete}</span>. Any commits only on
            it may become hard to find.
          </>
        }
        confirmLabel="Delete branch"
        onConfirm={() => branchToDelete && m.deleteBranch.mutate(branchToDelete)}
      />

      <RenameBranchDialog
        open={branchToRename !== null}
        onOpenChange={(o) => !o && renameBranchPrompt(null)}
        currentName={branchToRename ?? ''}
        existingNames={(branches.data?.local ?? []).map((b) => b.name)}
        hasUpstream={
          (branches.data?.local ?? []).find((b) => b.name === branchToRename)?.upstream != null
        }
        pending={m.renameBranch.isPending}
        onConfirm={(newName) =>
          branchToRename &&
          m.renameBranch.mutate(
            { name: branchToRename, newName },
            { onSuccess: () => renameBranchPrompt(null) }
          )
        }
      />

      <ConfirmDialog
        open={toDelete?.kind === 'tag'}
        onOpenChange={(o) => !o && setToDelete(null)}
        destructive
        title="Delete this tag?"
        description={
          <>
            This removes the tag{' '}
            <span className="font-mono text-foreground">{toDelete?.name}</span> from your local
            repository.
          </>
        }
        confirmLabel="Delete tag"
        onConfirm={() => toDelete && m.deleteTag.mutate(toDelete.name)}
      />
    </div>
  )
}
