import { useState, type ReactNode } from 'react'
import { FolderGit2, GitMerge, Sparkles } from 'lucide-react'
import { cn } from '@/lib/utils'
import { Button } from '@/components/ui/button'
import { Dialog, DialogContent, DialogTitle } from '@/components/ui/dialog'
import { useUiStore } from '@/stores/uiStore'

interface Slide {
  icon: ReactNode
  title: string
  body: ReactNode
}

const SLIDES: Slide[] = [
  {
    icon: <Sparkles size={22} strokeWidth={1.8} />,
    title: 'Welcome to GitWyrm',
    body: 'A fast, focused Git client for Windows. This quick tour shows the essentials — it takes about fifteen seconds.',
  },
  {
    icon: <FolderGit2 size={22} strokeWidth={1.8} />,
    title: 'Open your repositories',
    body: (
      <>
        Open a single repo, or point GitWyrm at your code folder (like{' '}
        <span className="font-mono text-foreground">C:\code</span>) to quick-launch everything
        inside it. Clone straight from a URL, too.
      </>
    ),
  },
  {
    icon: <GitMerge size={22} strokeWidth={1.8} />,
    title: 'Merge without the fear',
    body: (
      <>
        Hit <span className="font-semibold text-foreground">Merge</span> in the toolbar, or
        right-click any branch. When a merge hits conflicts, GitWyrm shows both sides side by side
        so you can keep yours, keep theirs, or edit the result — then commit.
      </>
    ),
  },
]

export function OnboardingModal() {
  const open = useUiStore((s) => s.activeModal === 'onboarding')
  const closeModal = useUiStore((s) => s.closeModal)
  const openModal = useUiStore((s) => s.openModal)
  const [step, setStep] = useState(0)

  const slide = SLIDES[step]
  const isLast = step === SLIDES.length - 1

  const finish = () => {
    setStep(0)
    openModal('clone')
  }

  const skip = () => {
    setStep(0)
    closeModal()
  }

  return (
    <Dialog open={open} onOpenChange={(o) => !o && skip()}>
      <DialogContent className="gap-0 p-0 sm:max-w-md" aria-describedby={undefined}>
        <div className="flex flex-col items-center px-6 pb-5 pt-8 text-center">
          <div className="mb-4 flex size-14 items-center justify-center rounded-2xl bg-soft text-accent-text">
            {slide.icon}
          </div>
          <DialogTitle className="text-base font-semibold text-foreground">
            {slide.title}
          </DialogTitle>
          <p className="mt-2 max-w-sm text-[0.78125rem] leading-relaxed text-sub">{slide.body}</p>

          <div className="mt-5 flex items-center gap-1.5">
            {SLIDES.map((_, i) => (
              <button
                key={i}
                onClick={() => setStep(i)}
                aria-label={`Go to step ${i + 1}`}
                className={cn(
                  'h-1.5 rounded-full transition-all',
                  i === step ? 'w-5 bg-primary' : 'w-1.5 bg-border hover:bg-muted-foreground'
                )}
              />
            ))}
          </div>
        </div>

        <div className="flex items-center justify-between border-t border-border px-4 py-3">
          <Button variant="ghost" size="sm" onClick={skip} className="text-sub">
            Skip
          </Button>
          <div className="flex gap-2">
            {step > 0 && (
              <Button variant="secondary" size="sm" onClick={() => setStep((s) => s - 1)}>
                Back
              </Button>
            )}
            {isLast ? (
              <Button size="sm" onClick={finish}>
                Open a repository
              </Button>
            ) : (
              <Button size="sm" onClick={() => setStep((s) => s + 1)}>
                Next
              </Button>
            )}
          </div>
        </div>
      </DialogContent>
    </Dialog>
  )
}
