import { useState } from 'react'
import { Check, ExternalLink, Plus } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { ConfirmDialog } from '@/components/modals/ConfirmDialog'
import { GithubIcon } from '@/components/domain/github/GithubIcon'
import { DeviceCodePanel } from '@/components/domain/github/DeviceCodePanel'
import {
  useGhCliStatus,
  useGithubMutations,
  useGithubSignIn,
  useHostAuthMutations,
  useHostingProviders,
} from '@/hooks/useGithub'
import type { HostProviderInfo, ProviderId } from '@/lib/bindings'
import { useWorkspaceStore } from '@/stores/workspaceStore'
import { SettingRow, SettingsGroup } from './SettingRow'
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
  /** Which host's connect form is open, or null when none is. */
  const [adding, setAdding] = useState<ProviderId | null>(null)

  const connected = providers.data?.filter((p) => p.connected_as != null) ?? []
  const available = providers.data?.filter((p) => p.connected_as == null) ?? []

  return (
    <div>
      <SettingsGroup title="Code hosts" blurb="Connect the sites where your repositories are stored.">
      {connected.map((provider) => (
        <ProviderRow key={provider.id} provider={provider} />
      ))}

      {/* Four hosts' worth of token boxes and scope lists is a wall of inputs
          for someone who wanted to connect one. Only the host being added shows
          its form; the rest stay a single row of buttons. */}
      {available.length > 0 && (
        <AddIntegration
          available={available}
          adding={adding}
          onPick={setAdding}
          onDone={() => setAdding(null)}
        />
      )}
      </SettingsGroup>

      {providers.isError && (
        <div className="py-3 text-2xs text-removed">
          Could not load the list of hosts. Close and reopen settings to try again.
        </div>
      )}
      <SettingsGroup title="Show on repository tabs">
        <TabCountSettings />
      </SettingsGroup>
      <ResetToDefaults group="integrations" />
    </div>
  )
}

/**
 * The unconnected hosts, as a row of buttons that each open one connect form.
 *
 * Deliberately not a dropdown: there are only ever a handful of hosts, and
 * seeing the names is how someone answers "is my host supported?" without
 * clicking anything.
 */
function AddIntegration({
  available,
  adding,
  onPick,
  onDone,
}: {
  available: HostProviderInfo[]
  adding: ProviderId | null
  onPick: (id: ProviderId | null) => void
  onDone: () => void
}) {
  const open = available.find((p) => p.id === adding)

  if (open) {
    return (
      <div className="border-t border-border pt-2">
        <ProviderRow provider={open} onConnected={onDone} />
        <div className="pb-3">
          <Button variant="ghost" size="sm" className="h-7 text-xs" onClick={() => onPick(null)}>
            Cancel
          </Button>
        </div>
      </div>
    )
  }

  return (
    <div className="border-t border-border py-4">
      <div className="text-xs font-semibold text-foreground">Add an integration</div>
      <div className="mt-0.5 text-2xs text-muted-foreground">
        Connect the site your code is hosted on to see its pull requests and issues in GitWyrm.
      </div>
      <div className="mt-2.5 flex flex-wrap gap-2">
        {available.map((provider) => (
          <Button
            key={provider.id}
            variant="secondary"
            size="sm"
            className="h-8 text-xs"
            onClick={() => onPick(provider.id)}
          >
            <Plus size={13} />
            {provider.display_name}
          </Button>
        ))}
      </div>
    </div>
  )
}

function ProviderRow({
  provider,
  onConnected,
}: {
  provider: HostProviderInfo
  /** Closes the add-integration form once the host accepts the credential. */
  onConnected?: () => void
}) {
  return provider.auth_kind === 'device_code' ? (
    <>
      <GithubConnection provider={provider} onConnected={onConnected} />
      <GhCliFallbackRow />
    </>
  ) : (
    <TokenConnection provider={provider} onConnected={onConnected} />
  )
}

/**
 * The GitHub CLI fallback.
 *
 * Some organizations block outside apps like GitWyrm from seeing their code,
 * which leaves the pull request and issue panels empty with nothing the user
 * can do about it from in here. The GitHub CLI is usually allowed where we are
 * not, so borrowing it fills those panels back in.
 *
 * The row says what the CLI's actual state is rather than only offering a
 * switch: "not installed" and "installed but not signed in" need different
 * fixes, and one vague "unavailable" would send half of the people reading it
 * to the wrong place.
 */
function GhCliFallbackRow() {
  const ghCliFallback = useWorkspaceStore((s) => s.ghCliFallback)
  const setGhCliFallback = useWorkspaceStore((s) => s.setGhCliFallback)
  const status = useGhCliStatus()

  const detail = !status.data
    ? null
    : status.data.signed_in
      ? 'GitHub CLI is ready. It will be used only when GitHub turns GitWyrm away.'
      : status.data.installed
        ? 'GitHub CLI is installed, but no one is signed in. Run "gh auth login" to use it.'
        : 'GitHub CLI is not installed, so there is nothing to fall back to yet.'

  return (
    <SettingRow
      label="Use the GitHub CLI as a backup"
      searchId="gh-cli-fallback"
      hint="Some organizations block outside apps. When that happens, GitWyrm can ask the GitHub CLI instead so pull requests and issues still show up."
    >
      <div className="grid gap-1.5">
        <label className="flex cursor-pointer items-center gap-2 text-xs text-foreground">
          <input
            type="checkbox"
            checked={ghCliFallback}
            onChange={(e) => setGhCliFallback(e.target.checked)}
            className="size-3.5 accent-[var(--gw-accent)]"
          />
          Ask the GitHub CLI when GitWyrm is turned away
        </label>
        {ghCliFallback && detail && (
          <div className="text-2xs text-muted-foreground">{detail}</div>
        )}
      </div>
    </SettingRow>
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

function TokenConnection({
  provider,
  onConnected,
}: {
  provider: HostProviderInfo
  onConnected?: () => void
}) {
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
          onConnected?.()
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
          ? `See and reply to ${provider.display_name} pull requests and issues in GitWyrm.`
          : `See ${provider.display_name} pull requests in GitWyrm.`
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

function GithubConnection({
  provider,
  onConnected,
}: {
  provider: HostProviderInfo
  onConnected?: () => void
}) {
  const signIn = useGithubSignIn(onConnected)
  const { signOut } = useGithubMutations(null)

  return (
    <SettingRow
      label={provider.display_name}
      searchId="github-connection"
      hint="See and reply to GitHub pull requests and issues in GitWyrm."
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
        hint="Show the open pull request count beside each connected repository."
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
        hint="Show the open issue count beside each connected repository."
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
