import { useState } from 'react'
import { Check } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { ConfirmDialog } from '@/components/modals/ConfirmDialog'
import { GithubIcon } from '@/components/domain/github/GithubIcon'
import { DeviceCodePanel } from '@/components/domain/github/DeviceCodePanel'
import { useGithubAuth, useGithubMutations, useGithubSignIn } from '@/hooks/useGithub'
import { useWorkspaceStore } from '@/stores/workspaceStore'
import { SettingRow } from './SettingRow'
import { ResetToDefaults } from './ResetToDefaults'

/**
 * Connections to the site your code is hosted on, and what those connections
 * are allowed to put on screen.
 *
 * Connecting used to be reachable only from the repository picker and the
 * sidebar, and disconnecting was not reachable at all -- `githubSignOut` existed
 * on both sides of the bridge with no button wired to it. Both live here now,
 * which is where someone goes looking when they want to change or revoke an
 * account.
 */
export function IntegrationsSettings() {
  return (
    <div>
      <GithubConnection />
      <TabCountSettings />
      <ResetToDefaults group="integrations" />
    </div>
  )
}

function GithubConnection() {
  const auth = useGithubAuth()
  const signIn = useGithubSignIn()
  const { signOut } = useGithubMutations(null)
  const [confirmingDisconnect, setConfirmingDisconnect] = useState(false)

  const login = auth.data ?? null

  return (
    <>
      <SettingRow
        label="GitHub"
        searchId="github-connection"
        hint="Lets GitWyrm show your pull requests and issues, and reply to them, without opening a browser. Your code is never sent anywhere -- this only reads the site."
      >
        {login ? (
          <div className="grid gap-2">
            <div className="flex items-center gap-2 text-xs text-foreground">
              <GithubIcon size={15} />
              <span className="font-medium">Connected as {login}</span>
              <Check size={13} className="text-accent-text" />
            </div>
            <div>
              <Button
                variant="secondary"
                size="sm"
                className="h-8 text-xs"
                onClick={() => setConfirmingDisconnect(true)}
                disabled={signOut.isPending}
              >
                {signOut.isPending ? 'Disconnecting…' : 'Disconnect'}
              </Button>
            </div>
          </div>
        ) : signIn.status.state === 'waiting' ? (
          <DeviceCodePanel
            userCode={signIn.status.userCode}
            verificationUri={signIn.status.verificationUri}
            onCancel={signIn.cancel}
          />
        ) : (
          <div className="grid gap-2">
            <div>
              <Button
                size="sm"
                className="h-8 text-xs"
                disabled={signIn.status.state === 'starting'}
                onClick={signIn.start}
              >
                <GithubIcon size={14} />
                {signIn.status.state === 'starting' ? 'Starting sign-in…' : 'Connect GitHub'}
              </Button>
            </div>
            {signIn.status.state === 'error' && (
              <div className="text-2xs text-removed">{signIn.status.message}</div>
            )}
          </div>
        )}
      </SettingRow>

      <ConfirmDialog
        open={confirmingDisconnect}
        onOpenChange={setConfirmingDisconnect}
        title="Disconnect GitHub?"
        description={
          <>
            GitWyrm will forget your GitHub sign-in. Pull requests and issues stop showing up, and
            the counts on your tabs go away. Nothing on GitHub changes, and none of your repositories
            or commits are touched. You can connect again any time.
          </>
        }
        confirmLabel="Disconnect"
        destructive
        onConfirm={() => signOut.mutate()}
      />
    </>
  )
}

/**
 * The two tab badges. Off by default because each one costs a request per
 * repository, which someone with a dozen tabs open should choose to spend.
 */
function TabCountSettings() {
  const showTabPrCount = useWorkspaceStore((s) => s.showTabPrCount)
  const setShowTabPrCount = useWorkspaceStore((s) => s.setShowTabPrCount)
  const showTabIssueCount = useWorkspaceStore((s) => s.showTabIssueCount)
  const setShowTabIssueCount = useWorkspaceStore((s) => s.setShowTabIssueCount)

  return (
    <>
      <SettingRow
        label="Pull requests on tabs"
        searchId="tab-pr-count"
        hint="Adds the number of open pull requests to each repository tab, beside the push and pull counts. Only repositories hosted on GitHub show one."
      >
        <label className="flex cursor-pointer items-center gap-2 text-xs text-foreground">
          <input
            type="checkbox"
            checked={showTabPrCount}
            onChange={(e) => setShowTabPrCount(e.target.checked)}
            className="size-3.5 accent-[var(--gw-accent)]"
          />
          Show open pull request count
        </label>
      </SettingRow>
      <SettingRow
        label="Issues on tabs"
        searchId="tab-issue-count"
        hint="Adds the number of open issues to each repository tab. Needs GitHub connected, because issues are not readable without signing in."
      >
        <label className="flex cursor-pointer items-center gap-2 text-xs text-foreground">
          <input
            type="checkbox"
            checked={showTabIssueCount}
            onChange={(e) => setShowTabIssueCount(e.target.checked)}
            className="size-3.5 accent-[var(--gw-accent)]"
          />
          Show open issue count
        </label>
      </SettingRow>
    </>
  )
}
