import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  Button,
} from 'gitwyrm-mockup'

export function DeleteBranch() {
  return (
    <Dialog defaultOpen modal={false}>
      <DialogContent
        onEscapeKeyDown={(e) => e.preventDefault()}
        onPointerDownOutside={(e) => e.preventDefault()}
      >
        <DialogHeader>
          <DialogTitle>Delete branch?</DialogTitle>
          <DialogDescription>
            This deletes <strong>feature/commit-graph</strong> locally. Commits that
            aren't on another branch will be lost.
          </DialogDescription>
        </DialogHeader>
        <DialogFooter>
          <Button variant="ghost">Cancel</Button>
          <Button variant="destructive">Delete branch</Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
