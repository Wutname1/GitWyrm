import { useEffect, useRef, useState } from 'react'
import { ChevronDown, GitBranch } from 'lucide-react'
import type { RefInfo, RefKind, RemoteInfo } from '@/lib/bindings'
import { detectProvider, providerLabel } from '@/lib/remoteProvider'
import { resolveDropPair, type DraggedRef } from '@/lib/refSync'
import { cn } from '@/lib/utils'
import { useBranches, useRemotes } from '@/hooks/useGitQueries'
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

function remoteName(refTag: RefInfo): string | null {
  return refTag.type === 'remote' ? refTag.name.split('/')[0] : null
}

function shortName(refTag: RefInfo): string {
  const remote = remoteName(refTag)
  return remote ? refTag.name.slice(remote.length + 1) : refTag.name
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
      const host = providerLabel(detectProvider(info?.url))
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
  const draggingRef = useDragStore((s) => s.draggingRef)
  const [open, setOpen] = useState(false)
  const closeTimer = useRef<number | null>(null)
  const pointerInside = useRef(false)
  const branchMenuOpen = useRef(false)

  const cancelClose = () => {
    if (closeTimer.current === null) return
    window.clearTimeout(closeTimer.current)
    closeTimer.current = null
  }

  const openStack = () => {
    cancelClose()
    setOpen(true)
  }

  const scheduleClose = () => {
    if (branchMenuOpen.current) return
    cancelClose()
    closeTimer.current = window.setTimeout(() => {
      closeTimer.current = null
      setOpen(false)
    }, 140)
  }

  useEffect(() => () => cancelClose(), [])

  const pointerEntered = () => {
    pointerInside.current = true
    openStack()
  }

  const pointerLeft = () => {
    pointerInside.current = false
    scheduleClose()
  }

  const branchMenuChanged = (nextOpen: boolean) => {
    branchMenuOpen.current = nextOpen
    if (nextOpen) {
      openStack()
    } else if (!pointerInside.current) {
      scheduleClose()
    }
  }

  const primary = refs.find((ref) => ref.type === 'head') ?? refs.find((ref) => ref.type === 'branch') ?? refs[0]
  const ordered = sortedRefs(refs, primary)
  const hiddenCount = refs.length - 1
  const label = shortName(primary)

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
        if (nextOpen) cancelClose()
        setOpen(nextOpen)
      }}
    >
      <PopoverTrigger asChild>
        <button
          type="button"
          aria-label={`${label} and ${hiddenCount} more ${hiddenCount === 1 ? 'name' : 'names'} on this commit`}
          aria-expanded={open}
          onClick={(event) => {
            event.preventDefault()
            event.stopPropagation()
            openStack()
          }}
          onPointerDown={(event) => event.stopPropagation()}
          onPointerEnter={pointerEntered}
          onPointerLeave={pointerLeft}
          onFocus={openStack}
          onBlur={scheduleClose}
          onDragEnter={() => {
            if (draggingRef && canAccept(draggingRef)) openStack()
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
          <span className="ml-1 flex h-full flex-none items-center gap-0.5 border-l border-primary-foreground/20 bg-black/10 px-1">
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
        onPointerEnter={pointerEntered}
        onPointerLeave={pointerLeft}
        className="w-72 overflow-hidden border-border bg-panel2 p-0 shadow-[0_12px_36px_rgba(0,0,0,0.5)]"
      >
        <div className="border-b border-border bg-panel px-3 py-2.5">
          <div className="font-medium text-foreground">{refs.length} labels on this commit</div>
          <div className="mt-0.5 text-2xs text-sub">Right-click a branch for options.</div>
        </div>
        <div className="relative max-h-64 overflow-y-auto px-3 py-2">
          <div aria-hidden className="absolute top-3 bottom-3 left-[19px] w-px bg-border" />
          <div className="space-y-1">
            {ordered.map((refTag) => {
              const source = sourceDetails(refTag, remotes.data ?? [])
              const row = (
                <div className="relative flex min-h-8 items-center gap-2.5 rounded-[5px] px-1.5 py-1 hover:bg-panel3">
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
              )
              const key = `${refTag.type}:${refTag.name}`
              return refTag.type === 'head' || refTag.type === 'branch' ? (
                <RefContextMenu key={key} refTag={refTag} onOpenChange={branchMenuChanged}>
                  {row}
                </RefContextMenu>
              ) : (
                <div key={key}>{row}</div>
              )
            })}
          </div>
        </div>
      </PopoverContent>
    </Popover>
  )
}
