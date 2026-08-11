import { useState } from 'react'
import { Check, ExternalLink } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { ConfirmDialog } from '@/components/modals/ConfirmDialog'
import { GithubIcon } from '@/components/domain/github/GithubIcon'
import { DeviceCodePanel } from '@/components/domain/github/DeviceCodePanel'
import {
  useGithubMutations,
  useGithubSignIn,
  useHostAuthMutations,
  useHostingProviders,
} from '@/hooks/useGithub'
import type { HostProviderInfo } from '@/lib/bindings'
import { useWorkspaceStore } from '@/stores/workspaceStore'
import { SettingRow } from './SettingRow'
import { ResetToDefaults } from './ResetToDefaults'

/**
 * Connections to the sites your code is hosted on, and what those connections
 * are allowed to put on screen.
 *
 * The provider list comes from the backend rather than being written out here,
 * so a host added to the Rust registry appears without touching this file. Each
 * row renders the connect control its host actually uses -- GitHub's device
 * code, a pasted token, or Bitbucket's email-plus-token pair.
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
  return provider.auth_kind === 'device_code' ? (
    <GithubConnection provider={provider} />
  ) : (
    <TokenConnection provider={provider} />
  )
}

/** Shared "connected as X, with a Disconnect button" block. */
function ConnectedState({
  provider,
  login,
  onDisconnect,
  pending,
}: {
  provider: HostProviderInfo
  login: string
  onDisconnect: () => void
  pending: boolean
}) {
  const [confirming, setConfirming] = useState(false)

  return (
    <>
      <div className="grid gap-2">
        <div className="flex items-center gap-2 text-xs text-foreground">
          {provider.auth_kind === 'device_code' && <GithubIcon size={15} />}
          <span className="font-medium">Connected as {login}</span>
          <Check size={13} className="text-accent-text" />
        </div>
        <div>
          <Button
            variant="secondary"
            size="sm"
            className="h-8 text-xs"
            onClick={() => setConfirming(true)}
            disabled={pending}
          >
            {pending ? 'Disconnecting…' : 'Disconnect'}
          </Button>
        </div>
      </div>

      <ConfirmDialog
        open={confirming}
        onOpenChange={setConfirming}
        title={`Disconnect ${provider.display_name}?`}
        description={
          <>
            GitWyrm will forget your {provider.display_name} sign-in. Pull requests and issues stop
            showing up, and the counts on your tabs go away. Nothing on {provider.display_name}{' '}
            changes, and none of your repositories or commits are touched. You can connect again any
            time.
          </>
        }
        confirmLabel="Disconnect"
        destructive
        onConfirm={onDisconnect}
      />
    </>
  )
}

/**
 * The permissions a token needs, listed verbatim.
 *
 * Worth the space: a token missing one scope is accepted when it is saved and
 * then fails on the first real request with an error that does not name the
 * cause. Showing the list next to the box is what stops that being a mystery.
 */
function ScopeHelp({ provider }: { provider: HostProviderInfo }) {
  if (provider.required_scopes.length === 0) return null
  return (
    <div className="text-2xs text-muted-foreground">
      Tick {provider.required_scopes.length === 1 ? 'this permission' : 'these permissions'} when you
      make the token:{' '}
      <span className="text-foreground">{provider.required_scopes.join(', ')}</span>
    </div>
  )
}

function TokenConnection({ provider }: { provider: HostProviderInfo }) {
  const { connect, disconnect } = useHostAuthMutations()
  const [token, setToken] = useState('')
  const [email, setEmail] = useState('')
  const [baseUrl, setBaseUrl] = useState('')

  const needsEmail = provider.auth_kind === 'email_and_token'
  // Only the self-hostable products get a base-URL box; Bitbucket Cloud and
  // dev.azure.com are fixed hosts for our purposes.
  const allowsSelfHosted = provider.id === 'gitlab' || provider.id === 'azure_devops'

  const submit = () => {
    connect.mutate(
      {
        provider: provider.id,
        token: token.trim(),
        email: needsEmail ? email.trim() : null,
        baseUrl: baseUrl.trim() || null,
      },
      {
        onSuccess: () => {
          setToken('')
          setEmail('')
        },
      }
    )
  }

  return (
    <SettingRow
      label={provider.display_name}
      searchId={`host-${provider.id}`}
      hint={
        provider.capabilities.issues
          ? `Show your ${provider.display_name} pull requests and issues in GitWyrm, and reply to them without opening a browser.`
          : `Show your ${provider.display_name} pull requests in GitWyrm. Work items are not shown, because every project defines its own.`
      }
    >
      {provider.connected_as ? (
        <ConnectedState
          provider={provider}
          login={provider.connected_as}
          pending={disconnect.isPending}
          onDisconnect={() => disconnect.mutate(provider.id)}
        />
      ) : (
        <div className="grid max-w-md gap-2">
          {needsEmail && (
            <Input
              value={email}
              onChange={(e) => setEmail(e.target.value)}
              placeholder="Your Atlassian account email"
              className="h-8 bg-background text-xs"
              autoComplete="off"
            />
          )}
          <Input
            type="password"
            value={token}
            onChange={(e) => setToken(e.target.value)}
            onKeyDown={(e) => e.key === 'Enter' && token.trim() && submit()}
            placeholder="Paste your token"
            className="h-8 bg-background font-mono text-xs"
            autoComplete="off"
          />
          {allowsSelfHosted && (
            <Input
              value={baseUrl}
              onChange={(e) => setBaseUrl(e.target.value)}
              placeholder={
                provider.id === 'gitlab'
                  ? 'Your own server address (leave blank for gitlab.com)'
                  : 'Your own server address (leave blank for dev.azure.com)'
              }
              className="h-8 bg-background text-xs"
              autoComplete="off"
            />
          )}
          <ScopeHelp provider={provider} />
          <div className="flex items-center gap-2">
            <Button
              size="sm"
              className="h-8 text-xs"
              disabled={!token.trim() || connect.isPending}
              onClick={submit}
            >
              {connect.isPending ? 'Checking…' : `Connect ${provider.display_name}`}
            </Button>
            {provider.token_url && (
              <Button
                variant="ghost"
                size="sm"
                className="h-8 text-xs"
                onClick={async () => {
                  const { openUrl } = await import('@tauri-apps/plugin-opener')
                  await openUrl(provider.token_url!)
                }}
              >
                <ExternalLink size={13} />
                Make a token
              </Button>
            )}
          </div>
        </div>
      )}
    </SettingRow>
  )
}

function GithubConnection({ provider }: { provider: HostProviderInfo }) {
  const signIn = useGithubSignIn()
  const { signOut } = useGithubMutations(null)

  return (
    <SettingRow
      label={provider.display_name}
      searchId="github-connection"
      hint="Lets GitWyrm show your pull requests and issues, and reply to them, without opening a browser. Your code is never sent anywhere -- this only reads the site."
    >
      {provider.connected_as ? (
        <ConnectedState
          provider={provider}
          login={provider.connected_as}
          pending={signOut.isPending}
          onDisconnect={() => signOut.mutate()}
        />
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
