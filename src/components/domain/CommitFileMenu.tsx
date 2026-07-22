import type { ReactNode } from 'react'
import { FileText } from 'lucide-react'
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuLabel,
  ContextMenuSeparator,
  ContextMenuTrigger,
} from '@/components/ui/context-menu'
import { FileActionMenuItems } from './FileActionMenuItems'

interface CommitFileMenuProps {
  path: string
  /** Commit these files belong to; blame opens pinned to it. */
  sha: string
  onOpen: () => void
  children: ReactNode
}

/**
 * Right-click menu for a file listed under a past commit. Staging and
 * discarding have no meaning here, so it offers only the actions that read:
 * open the file, show it in a folder, and the two history views.
 */
export function CommitFileMenu({ path, sha, onOpen, children }: CommitFileMenuProps) {
  const name = path.split('/').pop() ?? path

  return (
    <ContextMenu>
      <ContextMenuTrigger asChild>{children}</ContextMenuTrigger>
      <ContextMenuContent className="w-56">
        <ContextMenuLabel className="overflow-hidden text-ellipsis whitespace-nowrap font-mono text-2xs text-sub">
          {name}
        </ContextMenuLabel>
        <ContextMenuSeparator />
        <ContextMenuItem onSelect={onOpen}>
          <FileText />
          View changes
        </ContextMenuItem>
        <FileActionMenuItems path={path} sha={sha} />
      </ContextMenuContent>
    </ContextMenu>
  )
}
