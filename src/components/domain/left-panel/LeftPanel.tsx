import { type ReactNode, useEffect, useState } from 'react'
import { ArchiveRestore, ArrowLeftRight, CloudOff, ExternalLink, Eye, Tag, Trash2, Upload } from 'lucide-react'
import { formatCommitTime, formatRelativeTime } from '@/lib/gitDisplay'
import type { SectionItem, SidebarSectionData } from '@/lib/types'
import { useBranches, useCommitEntry, useRemotes, useStashes, useTags } from '@/hooks/useGitQueries'
import { useGitMutations } from '@/hooks/useGitMutations'
import { useTagSync } from '@/hooks/useTagSync'
import {
  useCommitPr,
  useGithubIssues,
  useGithubPrs,
  useGithubSlug,
  useHostingProviders,
  useRepoHostProvider,
} from '@/hooks/useGithub'
import { matchExplanation } from '@/lib/commitPr'
import { useUiStore } from '@/stores/uiStore'
import { useActiveRepo, useWorkspaceStore } from '@/stores/workspaceStore'
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuSeparator,
  ContextMenuTrigger,
} from '@/components/ui/context-menu'
import { ConfirmDialog } from '@/components/modals/ConfirmDialog'
import { InProgressModal } from '@/components/modals/InProgressModal'
import { classifyError } from '@/lib/errorClass'
import { RenameBranchDialog } from '@/components/modals/RenameBranchDialog'
import { branchSync } from '@/lib/branchActions'
import { tutorialBranchId } from '@/lib/tutorialLessons'
import { openWebUrl } from '@/lib/remoteWeb'
import { BranchMenu } from '@/components/domain/branch/BranchMenu'
import { StashContextMenu } from '@/components/domain/graph/StashRow'
import { SidebarSection } from './SidebarSection'
import { BranchSidebarItem } from './BranchSidebarItem'
import { RemotesSection } from './RemotesSection'
import { SpecsSection } from './SpecsSection'
import { useOpenspecChanges, useOpenspecStatus } from '@/hooks/useOpenspec'
import { openSpecDesk } from '@/lib/specDesk'
import { SubmodulesSection } from './SubmodulesSection'
import { WorktreesSection } from './WorktreesSection'

export function LeftPanel() {
  const repo = useActiveRepo()
  const selectCommit = useUiStore((s) => s.selectCommit)
  const revealRefInGraph = useUiStore((s) => s.revealRefInGraph)
  const revealShaInGraph = useUiStore((s) => s.revealShaInGraph)
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
  const openspecStatus = useOpenspecStatus(repo?.id ?? null)
  const openspecChanges = useOpenspecChanges(repo?.id ?? null)
  const specDeskEnabled = useWorkspaceStore((s) => s.enableSpecDesk)

  const githubSlug = useGithubSlug(repo?.id ?? null)
  // Which site this repository is actually on, and whether that one is
  // connected -- being signed in to GitHub says nothing about a GitLab repo.
  const repoHost = useRepoHostProvider(repo?.id ?? null)
  const providers = useHostingProviders()
  const host = providers.data?.find((p) => p.id === repoHost.data)
  const githubConnected = host?.connected_as != null
  const hostName = host?.display_name ?? 'your code host'
  const prs = useGithubPrs(githubSlug.data, githubConnected, repo?.id)
  const issues = useGithubIssues(githubSlug.data, githubConnected, repo?.id)
  const openGithubItem = useUiStore((s) => s.openGithubItem)
  const showSettings = useUiStore((s) => s.showSettings)

  // The pull request the selected commit belongs to, so its row in the list
  // below can say so. Reuses the same match the commit drawer shows, which is
  // what keeps the two from ever disagreeing about which one it is.
  const selectedSha = useUiStore((s) => s.selectedSha)
  const selectedCommit = useCommitEntry(repo?.id ?? null, selectedSha)
  const commitPr = useCommitPr(repo?.id ?? null, selectedCommit)

  const [toDelete, setToDelete] = useState<{ kind: 'branch' | 'tag'; name: string } | null>(null)
  /** Tag pending a remote-only delete; the local copy is untouched. */
  const [toRemoveFromRemote, setToRemoveFromRemote] = useState<string | null>(null)
  /** Opt-in to also removing the tag from the remote when deleting it here. */
  const [deleteTagFromRemote, setDeleteTagFromRemote] = useState(false)
  /** The delete that replaced the confirm dialog; `error` is null while working. */
  const [deleteRun, setDeleteRun] = useState<{
    name: string
    alsoRemote: boolean
    error: string | null
    friendly?: string
  } | null>(null)

  // The checkbox both reads and writes the saved preference, so whichever way
  // the user left it last is how it comes back. Subscribe to the app default and
  // this repo's override so the value stays reactive if either changes in
  // Settings > Tags while the dialog is closed.
  const appDeleteOnRemote = useWorkspaceStore((s) => s.tagDeleteOnRemote)
  const repoDeleteOnRemote = useWorkspaceStore((s) =>
    repo ? s.tagOverridesByRepo[repo.path]?.deleteOnRemote : undefined
  )
  const setTagDeleteOnRemote = useWorkspaceStore((s) => s.setTagDeleteOnRemote)
  const setRepoTagOverride = useWorkspaceStore((s) => s.setRepoTagOverride)
  const tagDeleteOnRemote = repoDeleteOnRemote ?? appDeleteOnRemote

  // Remember the choice for next time. A repo that has its own rule keeps it --
  // overwriting the app default from here would silently change every other
  // repository that follows it.
  const rememberDeleteFromRemote = (next: boolean) => {
    setDeleteTagFromRemote(next)
    if (repoDeleteOnRemote !== undefined && repo) {
      setRepoTagOverride(repo.path, { deleteOnRemote: next })
    } else {
      setTagDeleteOnRemote(next)
    }
  }
  const branchToRename = useUiStore((s) => s.branchToRename)
  const branchToDelete = useUiStore((s) => s.branchToDelete)
  const branchToResetTo = useUiStore((s) => s.branchToResetTo)
  const remoteBranchToDelete = useUiStore((s) => s.remoteBranchToDelete)
  const renameBranchPrompt = useUiStore((s) => s.renameBranchPrompt)
  const deleteBranchPrompt = useUiStore((s) => s.deleteBranchPrompt)
  const deleteRemoteBranchPrompt = useUiStore((s) => s.deleteRemoteBranchPrompt)
  const resetToBranchPrompt = useUiStore((s) => s.resetToBranchPrompt)

  // Opt-in to also deleting the local branch of the same name. Reset whenever a
  // different branch is queued up, so one branch's choice never carries over.
  const [alsoDeleteLocal, setAlsoDeleteLocal] = useState(false)
  useEffect(() => {
    setAlsoDeleteLocal(false)
  }, [remoteBranchToDelete?.remote, remoteBranchToDelete?.branch])

  // Only offer the local checkbox when a local branch of that name exists, and
  // never when it is the one checked out -- git cannot delete that.
  const remoteDeleteLocalCopy = remoteBranchToDelete
    ? branches.data?.local.find((b) => b.name === remoteBranchToDelete.branch)
    : undefined
  const canAlsoDeleteLocal = !!remoteDeleteLocalCopy && !remoteDeleteLocalCopy.is_head

  const currentBranch =
    branches.data?.local.find((b) => b.is_head)?.name ?? repo?.head_branch ?? ''

  // Only offer the remote checkbox when we have confirmed the tag is published;
  // an unknown status shouldn't invite an action that would just fail.
  const tagOnRemote =
    toDelete?.kind === 'tag' && tagSync.hasRemote && tagSync.stateOf(toDelete.name) === 'synced'

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
    // PR and issue sections only exist for repos on a host GitWyrm integrates
    // with. The issues section is dropped entirely for a host that has no
    // issue tracker (Azure DevOps), rather than showing a permanently empty one.
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
                  webUrl: p.html_url,
                  ...(commitPr?.number === p.number
                    ? { linked: true, linkedTitle: matchExplanation(commitPr) }
                    : {}),
                }))
              : [{ name: `Connect ${hostName}` }],
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
                  webUrl: i.html_url,
                }))
              : [{ name: `Connect ${hostName}` }],
          },
          // Filtered after the fact rather than spread in conditionally: a
          // conditional spread widens the literal types and `satisfies` stops
          // checking the section keys at all.
        ] satisfies SidebarSectionData[]).filter(
          (s) => s.key !== 'issues' || host?.capabilities.issues !== false
        )),
    {
      key: 'tags',
      label: 'TAGS',
      type: 'tag',
      // Only tags we have actually checked get the "not sent" marker; an
      // unknown status stays unmarked rather than guessing.
      items: (tags.data ?? []).map((t) => ({
        name: t.name,
        sha: t.target_sha,
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

  // Pull requests and issues come from the same host in one trip each, and
  // someone checking one almost always wants the other current too -- so the
  // button on either header reloads both.
  const refreshHostLists = () => {
    void prs.refetch()
    void issues.refetch()
  }
  const hostListsRefreshing = prs.isFetching || issues.isFetching
  const refreshAction: Partial<Record<string, { run: () => void; label: string; busy: boolean }>> =
    githubConnected
      ? {
          prs: {
            run: refreshHostLists,
            label: 'Refresh pull requests and issues',
            busy: hostListsRefreshing,
          },
          issues: {
            run: refreshHostLists,
            label: 'Refresh pull requests and issues',
            busy: hostListsRefreshing,
          },
        }
      : {}

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
        // GitHub signs in through the device-code modal; the token-based hosts
        // have their box in Settings, so send the user there instead of opening
        // a modal that cannot connect them.
        if (item.id == null) {
          if (repoHost.data === 'github') openModal('githubConnect')
          else showSettings('integrations')
        } else openGithubItem(section.type === 'pr' ? 'pr' : 'issue', item.id)
        break
      // Tags scroll to the commit they point at, matching branches and stashes.
      case 'tag':
        if (item.sha) revealShaInGraph(item.sha)
        break
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
    if ((section.type === 'pr' || section.type === 'issue') && item.id != null) {
      const githubKind = section.type === 'pr' ? 'pr' : 'issue'
      const kind = githubKind === 'pr' ? 'Pull request' : 'Issue'
      return (
        <ContextMenu>
          <ContextMenuTrigger asChild>{row}</ContextMenuTrigger>
          <ContextMenuContent className="w-52">
            <ContextMenuItem onSelect={() => openGithubItem(githubKind, item.id!)}>
              <Eye />
              View {kind.toLowerCase()}
            </ContextMenuItem>
            {item.webUrl && (
              <ContextMenuItem onSelect={() => openWebUrl(item.webUrl!, 'GitHub')}>
                <ExternalLink />
                View on GitHub
              </ContextMenuItem>
            )}
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
              onSelect={() => {
                // Start from the remembered choice each time the dialog opens.
                setDeleteTagFromRemote(tagDeleteOnRemote)
                setToDelete({ kind: 'tag', name: item.name })
              }}
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

  // Branch rows get their own drag-and-drop wiring so a branch can be dragged
  // onto another branch (or a graph chip) to sync/merge/reset. Other section
  // types fall through to the default row (return undefined).
  const renderBranchItem = (
    section: SidebarSectionData,
    item: SectionItem,
    ctx: { isCurrent: boolean; renderMenu: (row: ReactNode) => ReactNode }
  ): ReactNode | undefined => {
    if (section.type !== 'branch') return undefined
    return (
      <BranchSidebarItem
        section={section}
        item={item}
        currentBranch={currentBranch}
        isCurrent={ctx.isCurrent}
        onClick={() => onItemClick(section, item)}
        onDoubleClick={() => onItemDoubleClick(section, item)}
        hoverAction={getHoverAction(section, item)}
        pending={isItemPending(section, item)}
        disabled={isItemDisabled(section, item)}
        pendingLabel={getPendingLabel(section, item)}
        renderMenu={ctx.renderMenu}
        tutorialId={tutorialBranchId(item.name)}
      />
    )
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
    <div
      data-drag-scroll
      data-tutorial-id="branch-sidebar"
      className="h-full w-full overflow-y-auto border-r border-border bg-panel pb-6 pt-1.5"
    >
      <SidebarSection
        key={localSection.key}
        section={localSection}
        currentBranch={currentBranch}
        onItemClick={onItemClick}
        onItemDoubleClick={onItemDoubleClick}
        renderItemMenu={renderItemMenu}
        renderItem={renderBranchItem}
        onAdd={addAction.local?.run}
        addLabel={addAction.local?.label}
        onManage={() => openModal('branchManager')}
        manageLabel="Manage branches"
        isItemPending={isItemPending}
        isItemDisabled={isItemDisabled}
        getPendingLabel={getPendingLabel}
        getHoverAction={getHoverAction}
      />

      <RemotesSection remotes={remotes.data ?? []} onManage={() => openModal('remotes')} />

      <WorktreesSection />
      <SubmodulesSection />

      {/* Shown whenever Spec Desk is switched on, including in a repo with no
          openspec/ folder yet -- that repo is the one that needs the way in. */}
      {specDeskEnabled && (
        <SpecsSection
          changes={openspecChanges.data ?? []}
          hasOpenspec={openspecStatus.data?.present ?? false}
          repoId={repo.id}
          onOpenDesk={() => openSpecDesk(repo.id)}
          onNewChange={() => openModal('newChange')}
        />
      )}

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
          onRefresh={refreshAction[section.key]?.run}
          refreshLabel={refreshAction[section.key]?.label}
          refreshing={refreshAction[section.key]?.busy}
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

      <ConfirmDialog
        open={remoteBranchToDelete !== null}
        onOpenChange={(o) => !o && deleteRemoteBranchPrompt(null)}
        destructive
        title="Delete this branch from the server?"
        description={
          <>
            This removes{' '}
            <span className="font-mono text-foreground">
              {remoteBranchToDelete?.remote}/{remoteBranchToDelete?.branch}
            </span>{' '}
            from <span className="text-foreground">{remoteBranchToDelete?.remote}</span>, so{' '}
            <span className="text-removed">it disappears for everyone using it</span>. Anyone with a
            copy on their computer keeps it.
          </>
        }
        extra={
          canAlsoDeleteLocal ? (
            <label className="flex cursor-pointer items-start gap-2 text-xs text-sub">
              <input
                type="checkbox"
                checked={alsoDeleteLocal}
                onChange={(e) => setAlsoDeleteLocal(e.target.checked)}
                className="mt-0.5 size-3.5 accent-[var(--gw-accent)]"
              />
              <span>
                Also delete my copy{' '}
                <span className="font-mono text-foreground">{remoteBranchToDelete?.branch}</span>
                <span className="block text-2xs text-muted-foreground">
                  Commits only on it may become hard to find
                </span>
              </span>
            </label>
          ) : undefined
        }
        confirmLabel="Delete branch"
        pending={m.deleteRemoteBranch.isPending}
        pendingLabel="Deleting…"
        keepOpenOnConfirm
        onConfirm={() =>
          remoteBranchToDelete &&
          m.deleteRemoteBranch.mutate(
            {
              name: remoteBranchToDelete.branch,
              remote: remoteBranchToDelete.remote,
              alsoLocal: canAlsoDeleteLocal && alsoDeleteLocal,
            },
            { onSuccess: () => deleteRemoteBranchPrompt(null) }
          )
        }
      />

      <ConfirmDialog
        open={branchToResetTo !== null}
        onOpenChange={(o) => !o && resetToBranchPrompt(null)}
        destructive
        title={`Reset ${currentBranch || 'this branch'} to ${branchToResetTo}?`}
        description={
          <>
            This moves <span className="font-mono text-foreground">{currentBranch}</span> to match{' '}
            <span className="font-mono text-foreground">{branchToResetTo}</span> exactly and{' '}
            <span className="text-removed">erases any work you haven't committed</span>. Commits that
            were only on <span className="font-mono text-foreground">{currentBranch}</span> may become
            hard to find. This is hard to undo.
          </>
        }
        confirmLabel="Reset and erase"
        pending={m.resetToBranch.isPending}
        pendingLabel="Resetting…"
        keepOpenOnConfirm
        onConfirm={() =>
          branchToResetTo &&
          m.resetToBranch.mutate(
            { target: branchToResetTo, mode: 'Hard' },
            { onSuccess: () => resetToBranchPrompt(null) }
          )
        }
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
        extra={
          tagOnRemote ? (
            <label className="flex cursor-pointer items-start gap-2 text-xs text-sub">
              <input
                type="checkbox"
                checked={deleteTagFromRemote}
                onChange={(e) => rememberDeleteFromRemote(e.target.checked)}
                className="mt-0.5 size-3.5 accent-[var(--gw-accent)]"
              />
              <span>
                Also remove it from {tagSync.hostLabel}
                <span className="block text-2xs text-muted-foreground">
                  Anyone else using this project will lose it too.
                </span>
              </span>
            </label>
          ) : undefined
        }
        confirmLabel={
          tagOnRemote && deleteTagFromRemote ? 'Delete everywhere' : 'Delete tag'
        }
        onConfirm={() => {
          if (!toDelete) return
          const alsoRemote = tagOnRemote && deleteTagFromRemote
          // Hand off to the progress modal so the wait (and any failure) has
          // somewhere to live; the confirm dialog closes on its own.
          setDeleteRun({ name: toDelete.name, alsoRemote, error: null })
          m.deleteTag.mutate(
            { name: toDelete.name, alsoRemote, quiet: true },
            {
              onSuccess: () => setDeleteRun(null),
              onError: (reason) => {
                const { message: friendly, raw } = classifyError(reason)
                setDeleteRun((current) =>
                  current ? { ...current, error: raw, friendly } : current
                )
              },
            }
          )
        }}
      />

      <InProgressModal
        open={deleteRun != null}
        title={
          deleteRun?.alsoRemote
            ? `Deleting ${deleteRun.name} everywhere`
            : `Deleting tag ${deleteRun?.name ?? ''}`
        }
        subtext={
          deleteRun?.error != null
            ? deleteRun.friendly
            : deleteRun?.alsoRemote
              ? `Removing it from ${tagSync.hostLabel} too.`
              : 'This only takes a moment.'
        }
        error={deleteRun?.error ?? null}
        errorTitle="Could not delete the tag"
        onClose={() => {
          m.deleteTag.reset()
          setDeleteRun(null)
        }}
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
