import { useRef, useState, type ReactElement } from 'react'
import { ChevronDown, GitBranch } from 'lucide-react'
import type { RefInfo, RefKind, RemoteInfo } from '@/lib/bindings'
import { TooltipHint } from '@/components/ui/tooltip'
import { detectProvider, providerLabel } from '@/lib/remoteProvider'
import { resolveDropPair, type DraggedRef } from '@/lib/refSync'
import { MAX_INLINE_CHIPS, groupRefs, remoteName, shortName } from '@/lib/refStack'
import { cn } from '@/lib/utils'
import { useBranches, useRemotes } from '@/hooks/useGitQueries'
import { useGitMutations } from '@/hooks/useGitMutations'
import { useDragStore } from '@/stores/dragStore'
import { useActiveRepo } from '@/stores/workspaceStore'
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover'
import { RefBadge } from './RefBadge'
import { RefContextMenu } from './RefContextMenu'

const kindOrder: Record<RefKind, number> = {
  head: 0,
  branch: 1,
  remote: 2,
  tag: 3,
}

function groupKey(refTag: RefInfo): string {
  return refTag.type === 'tag' ? `tag:${refTag.name}` : `branch:${shortName(refTag)}`
}

function sortedRefs(refs: RefInfo[], primary: RefInfo): RefInfo[] {
  const primaryGroup = groupKey(primary)
  return [...refs].sort((a, b) => {
    const aPrimary = groupKey(a) === primaryGroup ? 0 : 1
    const bPrimary = groupKey(b) === primaryGroup ? 0 : 1
    return (
      aPrimary - bPrimary ||
      groupKey(a).localeCompare(groupKey(b)) ||
      kindOrder[a.type] - kindOrder[b.type] ||
      a.name.localeCompare(b.name)
    )
  })
}

function sourceDetails(refTag: RefInfo, remotes: RemoteInfo[]) {
  switch (refTag.type) {
    case 'head':
      return 'Current branch on this computer'
    case 'branch':
      return 'Branch on this computer'
    case 'tag':
      return 'Version tag'
    case 'remote': {
      const remote = remoteName(refTag)
      const info = remotes.find((item) => item.name === remote)
      const host = providerLabel(detectProvider(info))
      return host ? `From ${host} · ${remote}` : `From ${remote ?? 'a remote'}`
    }
  }
}

const markerStyles: Record<RefKind, string> = {
  head: 'bg-accent-text ring-accent-text/20',
  branch: 'bg-sub ring-sub/20',
  remote: 'bg-sub ring-sub/20',
  tag: 'bg-modified ring-modified/20',
}

export function RefStack({ refs }: { refs: RefInfo[] }) {
  const repo = useActiveRepo()
  const branches = useBranches(repo?.id ?? null)
  const remotes = useRemotes(repo?.id ?? null)
  const m = useGitMutations(repo?.id ?? null)
  const draggingRef = useDragStore((s) => s.draggingRef)
  const [open, setOpen] = useState(false)
  const branchMenuOpen = useRef(false)

  const branchMenuChanged = (nextOpen: boolean) => {
    branchMenuOpen.current = nextOpen
    if (nextOpen) setOpen(true)
  }

  // Same as double-clicking the chip itself -- see RefBadge for why a ref that
  // resolves to the branch you're already on fast-forwards instead of trying
  // (and failing) to switch to it.
  const currentBranch = branches.data?.local.find((b) => b.is_head)
  const localFor = (refTag: RefInfo) =>
    refTag.type === 'remote'
      ? (branches.data?.local.find((b) => b.upstream === refTag.name)?.name ??
        refTag.name.split('/').slice(1).join('/'))
      : refTag.name
  const willFastForward = (refTag: RefInfo) =>
    !!currentBranch && localFor(refTag) === currentBranch.name

  const switchTo = (refTag: RefInfo) => {
    if (m.checkout.isPending || m.fastForwardBranch.isPending) return
    if (willFastForward(refTag)) {
      m.fastForwardBranch.mutate({ branch: currentBranch!.name, target: refTag.name })
    } else {
      // The checkout mutation maps a remote-tracking ref onto its local branch,
      // so the full ref name works either way.
      m.checkout.mutate(refTag.name)
    }
    setOpen(false)
  }

  // Fold each local branch together with its own remote-tracking ref. A commit
  // sitting at the tip of two synced branches is two chips, not a four-row
  // popover -- there is no choice to make between `main` and `origin/main`.
  // Checked after the hooks above so hook order stays stable across renders.
  const upstreamOf = (localName: string) =>
    branches.data?.local.find((b) => b.name === localName)?.upstream
  const { groups, tags } = groupRefs(refs, upstreamOf)
  // Past a couple of chips the row stops being readable and the stack earns its
  // place back. Every group must have collapsed, too: an uncollapsed group is a
  // real choice, which is exactly what the popover is for.
  const allCollapsed = groups.every((g) => g.syncedWith)
  if (allCollapsed && groups.length > 0 && groups.length + tags.length <= MAX_INLINE_CHIPS) {
    return (
      <>
        {groups.map((group) => (
          <RefBadge
            key={`${group.primary.type}:${group.primary.name}`}
            refTag={group.primary}
            syncedWith={group.syncedWith ?? undefined}
            expandOnHover
          />
        ))}
        {tags.map((refTag) => (
          <RefBadge key={`${refTag.type}:${refTag.name}`} refTag={refTag} expandOnHover />
        ))}
      </>
    )
  }

  const primary = refs.find((ref) => ref.type === 'head') ?? refs.find((ref) => ref.type === 'branch') ?? refs[0]
  const ordered = sortedRefs(refs, primary)
  const hiddenCount = refs.length - 1
  const label = shortName(primary)

  // When the only things folded away are tags, tint the count segment with the
  // tag colour so the chip says "tags behind here" without opening it.
  const hidesOnlyTags = refs.every((refTag) => refTag === primary || refTag.type === 'tag')

  const canAccept = (dragged: DraggedRef) =>
    !!branches.data &&
    refs.some(
      (refTag) =>
        dragged.name !== refTag.name &&
        !!resolveDropPair(dragged, { name: refTag.name, type: refTag.type }, branches.data!)
    )

  return (
    <Popover
      open={open}
      onOpenChange={(nextOpen) => {
        if (!nextOpen && branchMenuOpen.current) return
        setOpen(nextOpen)
      }}
    >
      <PopoverTrigger asChild>
        <button
          type="button"
          aria-label={
            hidesOnlyTags
              ? `${label} and ${hiddenCount} ${hiddenCount === 1 ? 'tag' : 'tags'} on this commit`
              : `${label} and ${hiddenCount} more ${hiddenCount === 1 ? 'name' : 'names'} on this commit`
          }
          aria-expanded={open}
          onClick={(event) => {
            event.stopPropagation()
          }}
          onPointerDown={(event) => event.stopPropagation()}
          onDragEnter={() => {
            if (draggingRef && canAccept(draggingRef)) setOpen(true)
          }}
          className={cn(
            'inline-flex h-[19px] max-w-[138px] flex-none items-center overflow-hidden rounded-[5px] bg-primary font-mono text-2xs font-semibold leading-none text-primary-foreground outline-none transition-[filter,box-shadow]',
            'hover:brightness-110 focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-1 focus-visible:ring-offset-background'
          )}
        >
          <span className="flex min-w-0 items-center gap-1 py-px pl-1.5">
            <GitBranch aria-hidden className="size-2.5 flex-none" />
            <span className="overflow-hidden text-ellipsis whitespace-nowrap">{label}</span>
          </span>
          <span
            className={cn(
              'ml-1 flex h-full flex-none items-center gap-0.5 border-l px-1',
              hidesOnlyTags
                ? 'border-black/20 bg-lane1 text-[var(--gw-accent-fg)]'
                : 'border-primary-foreground/20 bg-black/10'
            )}
          >
            +{hiddenCount}
            <ChevronDown
              aria-hidden
              className={cn('size-2.5 transition-transform motion-reduce:transition-none', open && 'rotate-180')}
            />
          </span>
        </button>
      </PopoverTrigger>
      <PopoverContent
        align="start"
        sideOffset={6}
        onOpenAutoFocus={(event) => event.preventDefault()}
        onInteractOutside={(event) => {
          if (branchMenuOpen.current) event.preventDefault()
        }}
        onClick={(event) => event.stopPropagation()}
        onPointerDown={(event) => event.stopPropagation()}
        className="w-72 overflow-hidden border-border bg-panel2 p-0 shadow-[0_12px_36px_rgba(0,0,0,0.5)]"
      >
        <div className="border-b border-border bg-panel px-3 py-2.5">
          <div className="font-medium text-foreground">{refs.length} labels on this commit</div>
          <div className="mt-0.5 text-2xs text-sub">
            Double-click a branch to switch to it. Right-click for more options.
          </div>
        </div>
        <div className="relative max-h-64 overflow-y-auto px-3 py-2">
          <div aria-hidden className="absolute top-3 bottom-3 left-[19px] w-px bg-border" />
          <div className="space-y-1">
            {ordered.map((refTag) => {
              const source = sourceDetails(refTag, remotes.data ?? [])
              const canSwitch = refTag.type === 'branch' || refTag.type === 'remote'
              // The hint sits outside RefContextMenu, whose trigger is `asChild`
              // -- nested inside, the tooltip would become the trigger's target
              // and swallow the right-click menu.
              const withHint = (node: ReactElement) =>
                canSwitch ? (
                  <TooltipHint
                    label={
                      willFastForward(refTag)
                        ? `Double-click to catch ${currentBranch!.name} up to ${refTag.name}`
                        : `Double-click to switch to ${refTag.name}`
                    }
                  >
                    {node}
                  </TooltipHint>
                ) : (
                  node
                )
              return withHint(
                <RefContextMenu
                  key={`${refTag.type}:${refTag.name}`}
                  refTag={refTag}
                  onOpenChange={branchMenuChanged}
                >
                  <div
                    onDoubleClick={canSwitch ? () => switchTo(refTag) : undefined}
                    className={cn(
                      'relative flex min-h-8 items-center gap-2.5 rounded-[5px] px-1.5 py-1 hover:bg-panel3',
                      canSwitch && 'cursor-pointer'
                    )}
                  >
                    <span className="relative z-10 grid size-4 flex-none place-items-center rounded-full bg-panel2">
                      <span
                        aria-hidden
                        className={cn('size-1.5 rounded-full ring-4', markerStyles[refTag.type])}
                      />
                    </span>
                    <div className="min-w-0 flex-1">
                      <RefBadge refTag={refTag} withContextMenu={false} />
                      <div className="mt-0.5 overflow-hidden text-ellipsis whitespace-nowrap text-2xs text-sub">
                        {source}
                      </div>
                    </div>
                  </div>
                </RefContextMenu>
              )
            })}
          </div>
        </div>
      </PopoverContent>
    </Popover>
  )
}
