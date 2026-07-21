import { GithubIcon } from '@/components/domain/github/GithubIcon'
import { Button } from '@/components/ui/button'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import { useGithubSignIn } from '@/hooks/useGithub'
import { useUiStore } from '@/stores/uiStore'

/**
 * Device-code sign-in for the GitHub integration. Separate from the Copilot
 * sign-in in AI settings: this one asks for repo access so pull requests and
 * issues can be read and acted on.
 */
export function GithubConnectModal() {
  const open = useUiStore((s) => s.activeModal === 'githubConnect')
  const closeModal = useUiStore((s) => s.closeModal)
  const signIn = useGithubSignIn(closeModal)

  const onOpenChange = (next: boolean) => {
    if (!next) {
      signIn.cancel()
      closeModal()
    }
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="gap-0 p-0 sm:max-w-md" aria-describedby={undefined}>
        <DialogHeader className="border-b border-border px-4 pb-3 pt-4">
          <DialogTitle className="flex items-center gap-2 text-sm">
            <GithubIcon size={15} />
            Connect GitHub
          </DialogTitle>
        </DialogHeader>

        <div className="grid gap-3 px-4 py-4">
          <DialogDescription className="text-[12px] leading-relaxed text-sub">
            See and act on your pull requests and issues without leaving GitWyrm. GitHub will show
            you a code to confirm it is really you.
          </DialogDescription>

          {signIn.status.state === 'waiting' ? (
            <div className="rounded-md border border-border bg-background p-3 text-xs text-sub">
              Enter this code on GitHub:{' '}
              <span className="select-all font-mono text-base font-bold text-foreground">
                {signIn.status.userCode}
              </span>
              <div className="mt-2 flex items-center gap-2">
                <span className="text-[10.5px] text-muted-foreground">
                  Waiting for you to finish on GitHub…
                </span>
                <Button variant="ghost" size="sm" className="h-6 px-2 text-[10.5px]" onClick={signIn.cancel}>
                  Cancel
                </Button>
              </div>
            </div>
          ) : (
            <Button
              size="sm"
              className="h-8 text-xs"
              disabled={signIn.status.state === 'starting'}
              onClick={signIn.start}
            >
              {signIn.status.state === 'starting' ? 'Starting sign-in…' : 'Sign in with GitHub'}
            </Button>
          )}

          {signIn.status.state === 'error' && (
            <div className="text-[10.5px] text-removed">{signIn.status.message}</div>
          )}
        </div>
      </DialogContent>
    </Dialog>
  )
}
