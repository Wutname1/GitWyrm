import { Sparkles } from 'lucide-react'
import { AiSettings } from '@/components/domain/settings/AiSettings'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import { useUiStore } from '@/stores/uiStore'

/**
 * The AI settings panel, in a dialog.
 *
 * Every "AI settings…" in the app lands here rather than navigating to
 * Settings > AI. Setting up a provider is something you do in the middle of
 * something else -- writing a commit message, starting a spec run -- and a full
 * view change loses the thing you were doing. From the Spec Desk it used to be
 * worse than a view change: the Desk has no settings view of its own, so the
 * only answer was to bring the main window forward and leave the user in a
 * different window from the work.
 *
 * This renders the same `AiSettings` the settings page does, so the two can
 * never drift. Settings > AI stays where it is for anyone who navigates there.
 *
 * Not a `FormDialog`: there is nothing to submit. Every row saves as it is
 * changed, so the only footer this needs is a close button.
 */
export function AiSettingsModal() {
  const open = useUiStore((s) => s.activeModal === 'aiSettings')
  const closeModal = useUiStore((s) => s.closeModal)

  return (
    <Dialog open={open} onOpenChange={(next) => !next && closeModal()}>
      {/* Taller than the usual dialog and scrolling inside: the provider list,
          model picker, and instruction box together are longer than a short
          window, and clipping the instructions would hide the one row people
          come here to edit twice. */}
      <DialogContent className="flex max-h-[85vh] flex-col gap-0 p-0 sm:max-w-3xl">
        <DialogHeader className="flex-none border-b border-border px-6 py-4">
          <DialogTitle className="flex items-center gap-2 text-base">
            <Sparkles size={15} className="text-accent-text" />
            AI
          </DialogTitle>
          <DialogDescription className="text-2xs">
            Bring your own AI. Keys stay on this machine.
          </DialogDescription>
        </DialogHeader>
        <div className="min-h-0 flex-1 overflow-y-auto px-6 py-2">
          <AiSettings />
        </div>
      </DialogContent>
    </Dialog>
  )
}
