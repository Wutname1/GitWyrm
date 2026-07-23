import { FormDialog, Input } from 'gitwyrm-mockup'
import { GitBranch, Server } from 'lucide-react'

export function NewRemote() {
  return (
    <FormDialog
      open
      onOpenChange={() => {}}
      icon={<Server className="size-4 text-muted-foreground" />}
      title="Add a remote"
      submitLabel="Add remote"
      onSubmit={() => {}}
    >
      <label className="grid gap-1.5 text-xs text-muted-foreground">
        Name
        <Input defaultValue="origin" />
      </label>
      <label className="grid gap-1.5 text-xs text-muted-foreground">
        URL
        <Input defaultValue="git@github.com:gitwyrm/gitwyrm.git" />
      </label>
    </FormDialog>
  )
}

export function RenameBranch() {
  return (
    <FormDialog
      open
      onOpenChange={() => {}}
      icon={<GitBranch className="size-4 text-muted-foreground" />}
      title="Rename branch"
      submitLabel="Rename"
      onSubmit={() => {}}
    >
      <label className="grid gap-1.5 text-xs text-muted-foreground">
        New name
        <Input defaultValue="feature/commit-graph" />
      </label>
    </FormDialog>
  )
}

export function DeletePending() {
  return (
    <FormDialog
      open
      onOpenChange={() => {}}
      icon={<GitBranch className="size-4 text-muted-foreground" />}
      title="Delete branch"
      submitLabel="Delete branch"
      pendingLabel="Deleting..."
      pending
      destructive
      onSubmit={() => {}}
    >
      <p className="text-sm text-foreground">
        Delete <strong>experiment/inline-diff</strong> locally? Commits not on another
        branch will be lost.
      </p>
    </FormDialog>
  )
}
