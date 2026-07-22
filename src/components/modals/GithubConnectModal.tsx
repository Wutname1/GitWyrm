import { DeviceCodePanel } from '@/components/domain/github/DeviceCodePanel'
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
          <DialogDescription className="text-xs leading-relaxed text-sub">
            See and act on your pull requests and issues without leaving GitWyrm. We will give you a
            code to enter on GitHub, so it knows it is really you.
          </DialogDescription>

          {signIn.status.state === 'waiting' ? (
            <DeviceCodePanel
              userCode={signIn.status.userCode}
              verificationUri={signIn.status.verificationUri}
              onCancel={signIn.cancel}
            />
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
            <div className="text-2xs text-removed">{signIn.status.message}</div>
          )}
        </div>
      </DialogContent>
    </Dialog>
  )
}
