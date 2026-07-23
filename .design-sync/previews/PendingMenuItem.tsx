import {
  PendingMenuItem,
  ContextMenu,
  ContextMenuContent,
  ContextMenuTrigger,
  ContextMenuSeparator,
} from 'gitwyrm-mockup'
import { Download, GitBranch, Trash2, Upload } from 'lucide-react'

function MenuFrame({ children }: { children: React.ReactNode }) {
  return (
    <ContextMenu open modal={false}>
      <ContextMenuTrigger />
      <ContextMenuContent
        forceMount
        className="w-56"
        onEscapeKeyDown={(e) => e.preventDefault()}
        onPointerDownOutside={(e) => e.preventDefault()}
      >
        {children}
      </ContextMenuContent>
    </ContextMenu>
  )
}

export function Idle() {
  return (
    <div style={{ padding: 24 }}>
      <MenuFrame>
        <PendingMenuItem
          icon={<Download className="size-3.5" />}
          label="Pull from origin"
          pendingLabel="Pulling..."
          onRun={() => {}}
        />
        <PendingMenuItem
          icon={<Upload className="size-3.5" />}
          label="Push to origin"
          pendingLabel="Pushing..."
          onRun={() => {}}
        />
      </MenuFrame>
    </div>
  )
}

export function Pending() {
  return (
    <div style={{ padding: 24 }}>
      <MenuFrame>
        <PendingMenuItem
          icon={<Upload className="size-3.5" />}
          label="Push to origin"
          pendingLabel="Pushing..."
          pending
          onRun={() => {}}
        />
      </MenuFrame>
    </div>
  )
}

export function Destructive() {
  return (
    <div style={{ padding: 24 }}>
      <MenuFrame>
        <PendingMenuItem
          icon={<GitBranch className="size-3.5" />}
          label="Checkout branch"
          pendingLabel="Switching..."
          onRun={() => {}}
        />
        <ContextMenuSeparator />
        <PendingMenuItem
          icon={<Trash2 className="size-3.5" />}
          label="Delete branch"
          pendingLabel="Deleting..."
          variant="destructive"
          onRun={() => {}}
        />
      </MenuFrame>
    </div>
  )
}
