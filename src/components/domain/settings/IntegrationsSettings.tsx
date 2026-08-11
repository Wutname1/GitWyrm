import { useState } from 'react'
import { Check } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { ConfirmDialog } from '@/components/modals/ConfirmDialog'
import { GithubIcon } from '@/components/domain/github/GithubIcon'
import { DeviceCodePanel } from '@/components/domain/github/DeviceCodePanel'
import {
  useGithubMutations,
  useGithubSignIn,
  useHostingProviders,
} from '@/hooks/useGithub'
import type { HostProviderInfo } from '@/lib/bindings'
import { useWorkspaceStore } from '@/stores/workspaceStore'
import { SettingRow, settingRowClass, useRevealHighlight } from './SettingRow'
import { ResetToDefaults } from './ResetToDefaults'

/**
 * Connections to the sites your code is hosted on, and what those connections
 * are allowed to put on screen.
 *
 * Connecting used to be reachable only from the repository picker and the
 * sidebar, and disconnecting was not reachable at all -- `githubSignOut` existed
 * on both sides of the bridge with no button wired to it. Both live here now.
 *
 * The provider list comes from the backend rather than being written out here,
 * so a host added to the Rust registry appears without touching this file. Hosts
 * that are known but not built yet are listed and say so: someone whose work is
 * on GitLab gets a straight answer instead of finding GitHub alone and guessing
 * whether their host is unsupported or just undiscovered.
 */
export function IntegrationsSettings() {
  const providers = useHostingProviders()

  return (
    <div>
      {providers.data?.map((provider) => (
        <ProviderRow key={provider.id} provider={provider} />
      ))}
      {providers.isError && (
        <div className="py-3 text-2xs text-removed">
          Could not load the list of hosts. Close and reopen settings to try again.
        </div>
      )}
      <TabCountSettings />
      <ResetToDefaults group="integrations" />
    </div>
  )
}

function ProviderRow({ provider }: { provider: HostProviderInfo }) {
  if (!provider.implemented) return <PlannedProviderRow provider={provider} />
  // GitHub is the only implemented host today, and it is the only one with a
  // device-code flow wired up. When a second lands, switch on `auth_kind` here
  // rather than growing this branch.
  return <GithubConnection provider={provider} />
}

/**
 * A host GitWyrm knows the shape of but cannot talk to yet. Deliberately not a
 * disabled Connect button: a button that cannot ever work is a worse answer
 * than a sentence saying so.
 */
function PlannedProviderRow({ provider }: { provider: HostProviderInfo }) {
  const { ref, flash } = useRevealHighlight(`host-${provider.id}`)

  return (
    <div ref={ref} className={settingRowClass(flash)}>
      <div className="w-52 flex-none">
        <div className="text-xs font-semibold text-foreground">{provider.display_name}</div>
        <div className="mt-0.5 text-2xs text-muted-foreground">
          GitWyrm can open and work with repositories hosted here. Showing their pull requests and
          issues in the app is not built yet.
        </div>
      </div>
      <div className="min-w-0 flex-1">
        <span className="inline-flex items-center rounded-md border border-border px-2 py-1 text-2xs text-muted-foreground">
          Not available yet
        </span>
      </div>
    </div>
  )
}

function GithubConnection({ provider }: { provider: HostProviderInfo }) {
  const signIn = useGithubSignIn()
  const { signOut } = useGithubMutations(null)
  const [confirmingDisconnect, setConfirmingDisconnect] = useState(false)

  const login = provider.connected_as

  return (
    <>
      <SettingRow
        label={provider.display_name}
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
        hint="Adds the number of open pull requests to each repository tab, beside the push and pull counts. Only repositories on a connected host show one."
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
        hint="Adds the number of open issues to each repository tab. Needs a connected host, because issues are not readable without signing in."
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
