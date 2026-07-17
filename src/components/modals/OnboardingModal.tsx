import { Button } from '@/components/ui/button'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import { useUiStore } from '@/stores/uiStore'

export function OnboardingModal() {
  const open = useUiStore((s) => s.activeModal === 'onboarding')
  const closeModal = useUiStore((s) => s.closeModal)
  const openModal = useUiStore((s) => s.openModal)

  return (
    <Dialog open={open} onOpenChange={(o) => !o && closeModal()}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>Welcome to GitWyrm</DialogTitle>
          <DialogDescription>
            A fast, focused Git client. Open a repository — or point GitWyrm at your code folder
            to quick-launch everything in it.
          </DialogDescription>
        </DialogHeader>
        <DialogFooter>
          <Button variant="secondary" size="sm" onClick={closeModal}>
            Later
          </Button>
          <Button size="sm" onClick={() => openModal('clone')}>
            Open a repository
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
