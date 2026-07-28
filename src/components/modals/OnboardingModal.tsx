import { useEffect, useState, type ReactNode } from 'react'
import { FolderGit2, GitMerge, Hand, Loader2, ShieldCheck, Sparkles, UserRound } from 'lucide-react'
import { toast } from 'sonner'
import { cn } from '@/lib/utils'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Dialog, DialogContent, DialogTitle } from '@/components/ui/dialog'
import { commands, type SigningMethod } from '@/lib/bindings'
import { useGitIdentity } from '@/lib/useGitIdentity'
import { unwrap } from '@/lib/queryKeys'
import { useUiStore } from '@/stores/uiStore'
import { useTutorialStore } from '@/stores/tutorialStore'
import { TUTORIAL_ENABLED } from '@/lib/tutorialEnabled'
import { useWorkspaceStore } from '@/stores/workspaceStore'

interface Slide {
  icon: ReactNode
  title: string
  body: ReactNode
}

const TOUR_SLIDES: Slide[] = [
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

/**
 * First-launch tour, ending in the one thing GitWyrm cannot work without: the
 * name and email git puts on commits.
 *
 * Asking here is the difference between setting it up in ten seconds and
 * hitting a refusal at the first commit, which is where people used to meet it.
 */
export function OnboardingModal() {
  const open = useUiStore((s) => s.activeModal === 'onboarding')
  const closeModal = useUiStore((s) => s.closeModal)
  const showRepoPicker = useUiStore((s) => s.showRepoPicker)
  const markOnboardingSeen = useWorkspaceStore((s) => s.markOnboardingSeen)
  const [step, setStep] = useState(0)

  const { identity, isComplete } = useGitIdentity()

  // Someone who already has git set up does not need to be asked again, so the
  // identity step only exists when there is something to fill in. Frozen on
  // first read: saving during the step would otherwise remove the step the
  // user is standing on.
  const [needsIdentity, setNeedsIdentity] = useState<boolean | null>(null)
  useEffect(() => {
    if (identity !== null && needsIdentity === null) {
      setNeedsIdentity(!isComplete)
    }
  }, [identity, isComplete, needsIdentity])

  const slides = TOUR_SLIDES
  const identityStepIndex = needsIdentity ? slides.length : -1
  // The hands-on offer always comes last, after the identity is settled: it
  // opens a practice repo, and doing that before git knows who the user is
  // would leave them unable to commit in the very repo built for practising.
  // -1 drops the step entirely while the tour is off: no index can equal it, so
  // `onPracticeStep` is never true and the wizard ends on the step before it.
  const practiceStepIndex = !TUTORIAL_ENABLED
    ? -1
    : identityStepIndex >= 0
      ? identityStepIndex + 1
      : slides.length
  const lastIndex = TUTORIAL_ENABLED
    ? practiceStepIndex
    : identityStepIndex >= 0
      ? identityStepIndex
      : slides.length - 1
  const onIdentityStep = step === identityStepIndex
  const onPracticeStep = step === practiceStepIndex

  const finish = () => {
    setStep(0)
    markOnboardingSeen()
    closeModal()
    showRepoPicker()
  }

  /** Close the wizard and hand off to the tutorial instead of the repo picker. */
  const startPractice = () => {
    setStep(0)
    markOnboardingSeen()
    closeModal()
    void useTutorialStore.getState().start(async (path) => {
      const repo = unwrap(await commands.openRepo(path))
      useWorkspaceStore.getState().addRepo(repo)
      return repo.id
    })
  }

  const skip = () => {
    setStep(0)
    // Skipping still counts as seen: someone who dismissed the tour does not
    // want it again on the next launch.
    markOnboardingSeen()
    closeModal()
  }

  const slide = onIdentityStep || onPracticeStep ? null : slides[step]

  return (
    <Dialog open={open} onOpenChange={(o) => !o && skip()}>
      <DialogContent className="gap-0 p-0 sm:max-w-md" aria-describedby={undefined}>
        {onIdentityStep ? (
          <IdentityStep onDone={() => setStep(practiceStepIndex)} />
        ) : onPracticeStep ? (
          <PracticeStep onAccept={startPractice} onDecline={finish} />
        ) : (
          <div className="flex flex-col items-center px-6 pb-5 pt-8 text-center">
            <div className="mb-4 flex size-14 items-center justify-center rounded-2xl bg-soft text-accent-text">
              {slide?.icon}
            </div>
            <DialogTitle className="text-base font-semibold text-foreground">
              {slide?.title}
            </DialogTitle>
            <p className="mt-2 max-w-sm text-[0.78125rem] leading-relaxed text-sub">{slide?.body}</p>
          </div>
        )}

        <div className="flex items-center justify-center gap-1.5 pb-4">
          {Array.from({ length: lastIndex + 1 }, (_, i) => (
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
            {/* The identity and practice steps own their primary buttons: one
                saves, the other picks between two different destinations. */}
            {!onIdentityStep && !onPracticeStep && (
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

/**
 * The opt-in for the hands-on tour.
 *
 * Offered rather than started automatically, and phrased so declining is an
 * equal choice: someone who already knows git does not need five lessons on
 * gestures, and forcing them through it is a worse first impression than not
 * offering at all. The practice repo is named as throwaway right here, because
 * "we made you a repository" is alarming if you do not know it is disposable.
 */
function PracticeStep({
  onAccept,
  onDecline,
}: {
  onAccept: () => void
  onDecline: () => void
}) {
  const starting = useTutorialStore((s) => s.starting)

  return (
    <div className="flex flex-col items-center px-6 pb-5 pt-8 text-center">
      <div className="mb-4 flex size-14 items-center justify-center rounded-2xl bg-soft text-accent-text">
        <Hand size={22} strokeWidth={1.8} />
      </div>
      <DialogTitle className="text-base font-semibold text-foreground">
        Want to try the hidden shortcuts?
      </DialogTitle>
      <p className="mt-2 max-w-sm text-[0.78125rem] leading-relaxed text-sub">
        Some of the fastest things in GitWyrm are gestures: double-click, drag,
        right-click. We can set up a small practice repository and walk you
        through five of them, about two minutes. Nothing touches your own code,
        and it is deleted when you finish.
      </p>

      <Button size="sm" className="mt-5 h-9 w-full" onClick={onAccept} disabled={starting}>
        {starting ? (
          <>
            <Loader2 size={14} className="animate-spin" />
            Setting up practice...
          </>
        ) : (
          'Yes, show me'
        )}
      </Button>
      <Button
        variant="ghost"
        size="sm"
        className="mt-1.5 h-8 w-full text-sub"
        onClick={onDecline}
        disabled={starting}
      >
        No thanks, open a repository
      </Button>
    </div>
  )
}

/**
 * Name, email, and an optional signing key.
 *
 * Signing is offered rather than assumed: it is genuinely useful, but it is
 * also a concept a first-time user has not met yet, so it is one checkbox with
 * a plain-language explanation and no jargon.
 */
/**
 * Record what the wizard just set up as the user's first profile.
 *
 * Does not re-apply to git: `save()` above already wrote the identity, and the
 * signing key was made against it. Failing here is not worth interrupting the
 * wizard -- the identity is saved either way, and the Profiles screen offers to
 * adopt it later.
 */
async function recordProfile(name: string, email: string, signing: SigningMethod) {
  try {
    const seed = await commands.profileFromCurrentConfig()
    if (seed.status !== 'ok') return
    await commands.addProfileWithoutApplying({
      ...seed.data,
      // Named after the identity it holds; a profile called "My identity"
      // means nothing the moment there are two of them.
      label: email.trim(),
      name: name.trim(),
      email: email.trim(),
      signing,
      signCommits: signing.kind !== 'none',
    })
  } catch {
    // Profiles are a convenience here; git itself is already configured.
  }
}

function IdentityStep({ onDone }: { onDone: () => void }) {
  const { save } = useGitIdentity()
  const [name, setName] = useState('')
  const [email, setEmail] = useState('')
  const [wantSigning, setWantSigning] = useState(false)
  const [busy, setBusy] = useState(false)

  const ready = name.trim().length > 0 && email.trim().length > 0 && !busy

  const submit = async () => {
    if (!ready) return
    setBusy(true)

    const saved = await save(name, email)
    if (!saved.ok) {
      setBusy(false)
      toast.error(saved.error)
      return
    }

    let signing: SigningMethod = { kind: 'none' }
    if (wantSigning) {
      const made = await commands.createSigningKey(name, email)
      if (made.status !== 'ok') {
        // The identity saved, so this is a partial success, not a failure:
        // let them through and say what did not happen rather than trapping
        // them on the step.
        setBusy(false)
        toast.error(`Saved your name and email, but the signing key failed: ${made.error}`)
        await recordProfile(name, email, signing)
        onDone()
        return
      }
      signing = { kind: 'gpg', value: made.data }
      toast.success('Signing key ready. Turn it on for a repository in Settings > Security.')
    }

    // Leave the wizard with one profile rather than none, so the switcher has
    // something to show and a second identity is an "add" rather than a
    // "set the whole thing up".
    await recordProfile(name, email, signing)

    setBusy(false)
    onDone()
  }

  return (
    <div className="flex flex-col items-center px-6 pb-5 pt-8 text-center">
      <div className="mb-4 flex size-14 items-center justify-center rounded-2xl bg-soft text-accent-text">
        <UserRound size={22} strokeWidth={1.8} />
      </div>
      <DialogTitle className="text-base font-semibold text-foreground">
        Who should we put on your commits?
      </DialogTitle>
      <p className="mt-2 max-w-sm text-[0.78125rem] leading-relaxed text-sub">
        Every commit records who made it. Git needs this before it will save any work.
      </p>

      <div className="mt-4 grid w-full gap-2">
        <Input
          value={name}
          onChange={(e) => setName(e.target.value)}
          onKeyDown={(e) => e.key === 'Enter' && submit()}
          placeholder="Your name"
          className="h-9 bg-background text-xs"
          aria-label="Your name"
          autoFocus
        />
        <Input
          value={email}
          onChange={(e) => setEmail(e.target.value)}
          onKeyDown={(e) => e.key === 'Enter' && submit()}
          placeholder="you@example.com"
          className="h-9 bg-background text-xs"
          aria-label="Your email"
        />
      </div>

      <label className="mt-4 flex w-full cursor-pointer items-start gap-2 rounded-md border border-border bg-panel p-3 text-left">
        <input
          type="checkbox"
          checked={wantSigning}
          onChange={(e) => setWantSigning(e.target.checked)}
          className="mt-0.5 size-3.5 flex-none accent-[var(--gw-accent)]"
        />
        <span className="min-w-0">
          <span className="flex items-center gap-1.5 text-xs font-medium text-foreground">
            <ShieldCheck size={13} className="text-accent-text" />
            Also prove my commits are really mine
          </span>
          <span className="mt-0.5 block text-2xs leading-relaxed text-muted-foreground">
            Makes a signing key so hosts like GitHub can show a Verified badge on your work. You
            can turn this on later instead.
          </span>
        </span>
      </label>

      <Button size="sm" className="mt-4 h-9 w-full" onClick={submit} disabled={!ready}>
        {busy ? (
          <>
            <Loader2 size={14} className="animate-spin" />
            {wantSigning ? 'Setting things up...' : 'Saving...'}
          </>
        ) : (
          'Save and open a repository'
        )}
      </Button>
    </div>
  )
}
