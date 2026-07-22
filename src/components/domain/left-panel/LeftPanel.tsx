import { type ReactNode, useState } from 'react'
import { ArchiveRestore, ArrowLeftRight, CloudOff, Tag, Trash2, Upload } from 'lucide-react'
import { toast } from 'sonner'
import { formatCommitTime, formatRelativeTime } from '@/lib/gitDisplay'
import type { SectionItem, SidebarSectionData } from '@/lib/types'
import { useBranches, useRemotes, useStashes, useTags } from '@/hooks/useGitQueries'
import { useGitMutations } from '@/hooks/useGitMutations'
import { useTagSync } from '@/hooks/useTagSync'
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
import { StashContextMenu } from '@/components/domain/graph/StashRow'
import { SidebarSection } from './SidebarSection'
import { RemotesSection } from './RemotesSection'

export function LeftPanel() {
  const repo = useActiveRepo()
  const selectCommit = useUiStore((s) => s.selectCommit)
  const revealRefInGraph = useUiStore((s) => s.revealRefInGraph)
  const revealShaInGraph = useUiStore((s) => s.revealShaInGraph)
  const openMerge = useUiStore((s) => s.openMerge)
  const openNewTag = useUiStore((s) => s.openNewTag)
  const openModal = useUiStore((s) => s.openModal)
  const m = useGitMutations(repo?.id ?? null)

  const branches = useBranches(repo?.id ?? null)
  const tags = useTags(repo?.id ?? null)
  // The tag section is collapsed by default; don't reach for the network until
  // the user actually opens it.
  const tagsOpen = useUiStore((s) => s.sectionOpen.tags)
  const tagSync = useTagSync(repo?.id ?? null, tagsOpen)
  const remotes = useRemotes(repo?.id ?? null)
  const stashes = useStashes(repo?.id ?? null)

  const githubSlug = useGithubSlug(repo?.id ?? null)
  const githubAuth = useGithubAuth()
  const githubConnected = githubAuth.data != null
  const prs = useGithubPrs(githubSlug.data, githubConnected)
  const issues = useGithubIssues(githubSlug.data, githubConnected)
  const openGithubItem = useUiStore((s) => s.openGithubItem)

  const [toDelete, setToDelete] = useState<{ kind: 'branch' | 'tag'; name: string } | null>(null)
  /** Tag pending a remote-only delete; the local copy is untouched. */
  const [toRemoveFromRemote, setToRemoveFromRemote] = useState<string | null>(null)
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
      items: (stashes.data ?? []).map((s) => ({
        name: s.summary,
        sha: s.sha,
        meta: formatRelativeTime(s.time),
        metaTitle: `Stashed ${formatCommitTime(s.time)}${s.branch ? ` on ${s.branch}` : ''}`,
      })),
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
      // Only tags we have actually checked get the "not sent" marker; an
      // unknown status stays unmarked rather than guessing.
      items: (tags.data ?? []).map((t) => ({
        name: t.name,
        ...(tagSync.stateOf(t.name) === 'local'
          ? { meta: 'not sent', metaTitle: `Only on your computer. Send it to ${tagSync.hostLabel}.` }
          : {}),
      })),
    },
  ]

  // Section headers that get a hover `+` action, keyed by section key.
  const addAction: Partial<Record<string, { run: () => void; label: string }>> = {
    local: { run: () => openModal('newBranch'), label: 'New branch' },
    tags: { run: () => openNewTag(), label: 'New tag' },
  }

  const stashBySha = (sha?: string) => (stashes.data ?? []).find((s) => s.sha === sha)
  const stashBusy = m.stashPop.isPending || m.stashApply.isPending || m.stashDrop.isPending

  const isItemPending = (section: SidebarSectionData, item: SectionItem) =>
    (section.type === 'branch' && m.checkout.isPending && m.checkout.variables === item.name) ||
    (section.type === 'stash' &&
      stashBusy &&
      stashBySha(item.sha)?.index ===
        (m.stashPop.isPending
          ? m.stashPop.variables
          : m.stashApply.isPending
            ? m.stashApply.variables
            : m.stashDrop.variables))

  const isItemDisabled = (section: SidebarSectionData, item: SectionItem) =>
    isItemPending(section, item) ||
    (section.type === 'branch' && m.checkout.isPending) ||
    (section.type === 'stash' && stashBusy)

  const getPendingLabel = (section: SidebarSectionData, item: SectionItem) =>
    section.type === 'branch'
      ? `Switching to ${item.name}…`
      : m.stashDrop.isPending
        ? 'Deleting stash…'
        : 'Applying stash…'

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
      // A click only shows the stash (scroll to its graph row, files in the
      // drawer). Applying is an explicit action: hover icon or right-click.
      case 'stash':
        if (item.sha) revealShaInGraph(item.sha)
        break
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

  // A quick-switch icon appears on hover for branches other than the current
  // one; stashes get an apply icon.
  const getHoverAction = (section: SidebarSectionData, item: SectionItem) => {
    if (section.type === 'branch' && item.name !== currentBranch) {
      return {
        icon: <ArrowLeftRight size={12} strokeWidth={2.2} />,
        title: `Switch to ${item.name}`,
        onClick: () => switchToBranch(item.name),
      }
    }
    if (section.type === 'stash') {
      return {
        icon: <ArchiveRestore size={12} strokeWidth={2.2} />,
        title: 'Apply and remove stash',
        onClick: () => {
          const stash = stashBySha(item.sha)
          if (stash != null && !stashBusy) m.stashPop.mutate(stash.index)
        },
      }
    }
    return undefined
  }

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
    if (section.type === 'stash') {
      const stash = stashBySha(item.sha)
      if (stash == null) return null
      return <StashContextMenu stash={stash}>{row}</StashContextMenu>
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
            {tagSync.hasRemote && tagSync.stateOf(item.name) === 'local' && (
              <ContextMenuItem onSelect={() => m.pushTag.mutate({ name: item.name })}>
                <Upload />
                Send to {tagSync.hostLabel}
              </ContextMenuItem>
            )}
            <ContextMenuSeparator />
            {tagSync.hasRemote && tagSync.stateOf(item.name) === 'synced' && (
              <ContextMenuItem
                variant="destructive"
                onSelect={() => setToRemoveFromRemote(item.name)}
              >
                <CloudOff />
                Remove from {tagSync.hostLabel}
              </ContextMenuItem>
            )}
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
      <div className="h-full w-full border-r border-border bg-panel p-4 text-xs text-muted-foreground">
        No repository open
      </div>
    )
  }

  const localSection = sections[0]
  const otherSections = sections.slice(1)

  return (
    <div data-dim-on-drag className="h-full w-full overflow-y-auto border-r border-border bg-panel pb-6 pt-1.5">
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

      <ConfirmDialog
        open={toRemoveFromRemote != null}
        onOpenChange={(o) => !o && setToRemoveFromRemote(null)}
        destructive
        title={`Remove this tag from ${tagSync.hostLabel}?`}
        description={
          <>
            This removes <span className="font-mono text-foreground">{toRemoveFromRemote}</span> from{' '}
            {tagSync.hostLabel}, where anyone else using this project will lose it. Your own copy
            stays.
          </>
        }
        confirmLabel="Remove it"
        pending={m.deleteRemoteTag.isPending}
        pendingLabel="Removing…"
        onConfirm={() =>
          toRemoveFromRemote && m.deleteRemoteTag.mutate({ name: toRemoveFromRemote })
        }
      />
    </div>
  )
}
