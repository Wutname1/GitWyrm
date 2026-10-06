import { useState } from 'react'
import { AlertTriangle, Check, ExternalLink, Plus } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { ConfirmDialog } from '@/components/modals/ConfirmDialog'
import { groupHostingProviders } from '@/lib/hostingGroups'
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
import mehenMark from '@/assets/mehen-mark.png'
import { openInMehen, useMehenOverview } from '@/hooks/useMehen'
import { formatRelativeTime } from '@/lib/gitDisplay'
import { MEHEN_TAB_LEVELS, parseMehenTabLevel } from '@/lib/mehen'
import { cn } from '@/lib/utils'
import { useActiveRepo, useWorkspaceStore } from '@/stores/workspaceStore'
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

  // Three groups, not two: a saved sign-in that failed is neither connected
  // nor available. See `groupHostingProviders` for why that distinction is
  // load-bearing.
  const { connected, failing, available } = groupHostingProviders(providers.data)

  return (
    <div>
      <SettingsGroup title="Code hosts" blurb="Connect the sites where your repositories are stored.">
      {connected.map((provider) => (
        <ProviderRow key={provider.id} provider={provider} />
      ))}

      {failing.map((provider) => (
        <NeedsAttention key={provider.id} provider={provider} />
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
      <SettingsGroup title="Mehen" blurb="Mehen checks your projects' packages for known security problems. GitWyrm shows what it found.">
        <MehenSettings />
      </SettingsGroup>
      <ResetToDefaults group="integrations" />
    </div>
  )
}

/**
 * A host that is set up but whose sign-in did not work.
 *
 * Amber rather than red, and worded as a thing to finish rather than a thing
 * that broke: the usual cause is a token that expired or was revoked, which is
 * ordinary and fixed in a minute. Shows the host's own words underneath,
 * because "could not sign in" alone does not tell anyone which of the several
 * possible causes they have.
 */
function NeedsAttention({ provider }: { provider: HostProviderInfo }) {
  return (
    <div className="grid gap-2 rounded-md border border-amber-500/40 bg-amber-500/10 px-3 py-2.5">
      <div className="flex items-center gap-2">
        <AlertTriangle size={14} className="flex-none text-amber-600 dark:text-amber-300" aria-hidden />
        <span className="text-xs font-medium text-foreground">
          {provider.display_name} needs signing in again
        </span>
      </div>
      <p className="text-2xs leading-relaxed text-muted-foreground">
        GitWyrm has a sign-in saved for {provider.display_name} but could not use it. This usually
        means it expired or was turned off.
      </p>
      {provider.auth_error && (
        <p className="rounded bg-panel3 px-2 py-1 font-mono text-[10.5px] leading-snug text-muted-foreground">
          {provider.auth_error}
        </p>
      )}
      <div>
        <ProviderRow provider={provider} />
      </div>
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

/** One checkbox row, in the style of the tab count settings above. */
function MehenToggle({
  label,
  searchId,
  hint,
  text,
  checked,
  disabled,
  onChange,
}: {
  label: string
  searchId: string
  hint: string
  text: string
  checked: boolean
  disabled?: boolean
  onChange: (enabled: boolean) => void
}) {
  return (
    <SettingRow label={label} searchId={searchId} hint={hint}>
      <label className={cn('flex items-center gap-2 text-xs text-foreground', disabled ? 'cursor-not-allowed opacity-50' : 'cursor-pointer')}>
        <input
          type="checkbox"
          checked={checked}
          disabled={disabled}
          onChange={(e) => onChange(e.target.checked)}
          className="size-3.5 accent-[var(--gw-accent)]"
        />
        {text}
      </label>
    </SettingRow>
  )
}

/**
 * What GitWyrm does with Mehen, the dependency checker. Everything here only
 * shows Mehen's own answers; GitWyrm never checks packages itself. Mehen's
 * daily check with the app closed is Mehen's own setting, so it lives there.
 */
function MehenSettings() {
  const { data: overview, isLoading } = useMehenOverview()
  const activeRepo = useActiveRepo()
  const showStatus = useWorkspaceStore((s) => s.mehenShowStatus)
  const setShowStatus = useWorkspaceStore((s) => s.setMehenShowStatus)
  const tabLevel = useWorkspaceStore((s) => s.mehenTabLevel)
  const setTabLevel = useWorkspaceStore((s) => s.setMehenTabLevel)
  const keepFresh = useWorkspaceStore((s) => s.mehenKeepFresh)
  const setKeepFresh = useWorkspaceStore((s) => s.setMehenKeepFresh)
  const newFixNotes = useWorkspaceStore((s) => s.mehenNewFixNotes)
  const setNewFixNotes = useWorkspaceStore((s) => s.setMehenNewFixNotes)

  const status = isLoading
    ? 'Looking for Mehen…'
    : !overview
      ? "Mehen hasn't checked your projects on this computer yet. Install Mehen and add your code folder, and its results show up here."
      : overview.full_check_at
        ? `Mehen last checked all your projects ${formatRelativeTime(overview.full_check_at)}. It watches ${overview.repos.length} ${overview.repos.length === 1 ? 'repository' : 'repositories'}.`
        : `Mehen watches ${overview.repos.length} ${overview.repos.length === 1 ? 'repository' : 'repositories'}.`

  return (
    <>
      <SettingRow label="Mehen" searchId="mehen-status" hint={status}>
        {overview?.can_open && activeRepo && (
          <Button variant="outline" size="sm" onClick={() => void openInMehen(activeRepo.id, false)}>
            <img src={mehenMark} alt="" className="size-4" draggable={false} />
            Open in Mehen
          </Button>
        )}
      </SettingRow>
      <MehenToggle
        label="Status bar"
        searchId="mehen-show-status"
        hint="Show what Mehen flags for the open repository in the status bar, and mention security fixes when you push changes to your packages."
        text="Show in the status bar"
        checked={showStatus}
        onChange={setShowStatus}
      />
      <SettingRow
        label="What Mehen flags"
        searchId="mehen-tab-level"
        hint="What the tab badge, the status bar and the Mehen list in the sidebar count. Each choice includes everything above it. Security problems count only when they have a fix."
      >
        <select
          className="h-8 w-full rounded-md border border-input bg-background px-2 text-xs text-foreground outline-none focus:border-ring"
          value={tabLevel}
          onChange={(e) => setTabLevel(parseMehenTabLevel(e.target.value))}
          aria-label="What Mehen flags"
        >
          {MEHEN_TAB_LEVELS.map((level) => (
            <option key={level.id} value={level.id}>
              {level.label}
            </option>
          ))}
        </select>
      </SettingRow>
      <MehenToggle
        label="Keep Mehen's results up to date"
        searchId="mehen-keep-fresh"
        hint="When Mehen's results are more than 12 hours old, or a pull changes a project's packages, Mehen checks again in the background. Its window does not open."
        text="Check in the background"
        checked={keepFresh}
        onChange={setKeepFresh}
      />
      <MehenToggle
        label="New security fixes"
        searchId="mehen-new-fix-notes"
        hint="A note, once, when Mehen finds a new fix for a project you have open."
        text="Tell me about new fixes"
        checked={newFixNotes}
        onChange={setNewFixNotes}
      />
    </>
  )
}
