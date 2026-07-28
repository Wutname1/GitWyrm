import { useEffect, useState } from 'react'
import { PartyPopper, TriangleAlert } from 'lucide-react'
import { toast } from 'sonner'
import { Button } from '@/components/ui/button'
import { Dialog, DialogContent, DialogTitle } from '@/components/ui/dialog'
import { commands } from '@/lib/bindings'
import { LESSONS } from '@/lib/tutorialLessons'
import { useTutorialStore } from '@/stores/tutorialStore'
import { useUiStore } from '@/stores/uiStore'
import { useWorkspaceStore } from '@/stores/workspaceStore'

/**
 * The card shown when the tutorial ends, whichever way it ended.
 *
 * Its real job is cleanup: the practice repo has to go away, and this is the
 * one place that reliably runs on both paths (finished and bailed out). Doing
 * the deletion here rather than on unmount means the user watches it happen and
 * gets told, instead of a folder quietly vanishing.
 */
export function TutorialOutcomeDialog() {
  const outcome = useTutorialStore((s) => s.outcome)
  const error = useTutorialStore((s) => s.error)
  const completed = useTutorialStore((s) => s.completed)
  const dismissOutcome = useTutorialStore((s) => s.dismissOutcome)
  const cleanup = useTutorialStore((s) => s.cleanup)
  const showRepoPicker = useUiStore((s) => s.showRepoPicker)
  const [busy, setBusy] = useState(false)

  // A failure to seed never gets an outcome card -- there is nothing to clean
  // up and nothing was practised -- so it is surfaced as a toast and cleared.
  useEffect(() => {
    if (!error) return
    toast.error(`Could not start the practice tour: ${error}`)
    dismissOutcome()
  }, [error, dismissOutcome])

  if (!outcome) return null

  const done = completed.size
  const total = LESSONS.length

  const finishUp = async () => {
    setBusy(true)
    await cleanup(async (repoId) => {
      useWorkspaceStore.getState().removeRepo(repoId)
      await commands.closeRepo(repoId)
    })
    setBusy(false)
    dismissOutcome()
    // Land them where the tour would have sent them anyway: ready to open
    // something real.
    showRepoPicker()
  }

  return (
    <Dialog open onOpenChange={(o) => !o && void finishUp()}>
      <DialogContent className="gap-0 p-0 sm:max-w-sm" aria-describedby={undefined}>
        <div className="flex flex-col items-center px-6 pb-5 pt-8 text-center">
          <div className="mb-4 flex size-14 items-center justify-center rounded-2xl bg-soft text-accent-text">
            {outcome === 'finished' ? (
              <PartyPopper size={22} strokeWidth={1.8} />
            ) : (
              <TriangleAlert size={22} strokeWidth={1.8} />
            )}
          </div>
          <DialogTitle className="text-base font-semibold text-foreground">
            {outcome === 'finished' ? 'That is the tour' : 'Stopped the tour'}
          </DialogTitle>
          <p className="mt-2 max-w-sm text-[0.78125rem] leading-relaxed text-sub">
            {done === total
              ? 'You tried all five. Those same gestures work everywhere in GitWyrm, not just in the practice repository.'
              : done > 0
                ? `You tried ${done} of ${total}. You can run the tour again any time from Settings.`
                : 'No problem. You can run the tour any time from Settings.'}
          </p>
          <p className="mt-3 text-2xs leading-relaxed text-muted-foreground">
            The practice repository will be deleted now.
          </p>

          <Button size="sm" className="mt-4 h-9 w-full" onClick={finishUp} disabled={busy}>
            {busy ? 'Cleaning up...' : 'Open a real repository'}
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  )
}
