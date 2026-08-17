import { useCallback, useEffect, useState } from 'react'
import {
  AlertTriangle,
  Check,
  Copy,
  ExternalLink,
  KeyRound,
  Loader2,
  ShieldCheck,
  Sparkles,
  Trash2,
} from 'lucide-react'
import { toast } from 'sonner'
import { ConfirmDialog } from '@/components/modals/ConfirmDialog'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { commands, type SigningKey, type SigningStatus } from '@/lib/bindings'
import { executableName } from '@/lib/platform'
import { cn } from '@/lib/utils'
import { useGitIdentity } from '@/lib/useGitIdentity'
import { useActiveRepo, useWorkspaceStore } from '@/stores/workspaceStore'
import { PublishSteps } from './PublishSteps'
import { SettingRow } from './SettingRow'
import { SshAccess } from './SshAccess'
import { ToolStatusRow, useToolCheck, type ToolState } from './ToolStatusRow'

/**
 * Security settings: signing commits, and the git and gpg GitWyrm uses to do
 * it.
 *
 * Signing normally means installing GnuPG, generating a key on the command
 * line, and pasting a fingerprint into a config file. GitWyrm ships its own
 * gpg and does all of that in one button, because the people this app is for
 * will not do it otherwise (Rule #2).
 */
export function SecuritySettings() {
  const repo = useActiveRepo()

  return (
    <div className="grid gap-8">
      <SigningSection repoPath={repo?.path ?? null} />
      <Section
        title="Connecting over SSH"
        blurb="How this computer proves who it is when it pushes and pulls. Separate from signing: this is about getting access, not about proving a commit is yours."
      >
        <SshAccess />
      </Section>
      <ToolsSection />
    </div>
  )
}

/* ------------------------------------------------------------------ signing */

function SigningSection({ repoPath }: { repoPath: string | null }) {
  const [status, setStatus] = useState<SigningStatus | null>(null)
  const [loading, setLoading] = useState(true)

  // Loaded directly rather than through useToolCheck: that hook is for the
  // git/gpg version probes and takes a fresh deps array on every render, so it
  // could not tell a real refresh from a re-render. Making a key then left the
  // panel showing pre-key state even though the backend had done the work.
  const load = useCallback(async () => {
    if (!repoPath) {
      setStatus(null)
      setLoading(false)
      return
    }
    setLoading(true)
    const res = await commands.getSigningStatus(repoPath)
    setLoading(false)
    if (res.status === 'ok') {
      setStatus(res.data)
    } else {
      setStatus(null)
      toast.error(res.error)
    }
  }, [repoPath])

  useEffect(() => {
    void load()
  }, [load])

  /** Re-read after anything that changes signing state. */
  const refresh = useCallback(() => {
    void load()
  }, [load])

  if (!repoPath) {
    return (
      <Section
        title="Signing"
        blurb="Signing proves a commit really came from you. Open a repository to set it up."
      >
        <div className="rounded-md border border-border bg-panel px-3 py-6 text-center text-xs text-muted-foreground">
          Open a repository to turn on signing.
        </div>
      </Section>
    )
  }

  if (loading && !status) {
    return (
      <Section title="Signing" blurb="Signing proves a commit really came from you.">
        <div className="flex items-center gap-2 px-1 py-4 text-xs text-muted-foreground">
          <Loader2 size={14} className="animate-spin" />
          Checking your signing setup...
        </div>
      </Section>
    )
  }

  return (
    <Section
      title="Signing"
      blurb="Signing proves a commit really came from you. Hosts like GitHub show a Verified badge next to signed commits."
    >
      {((status?.brokenFormat !== null && status?.brokenFormat !== undefined) ||
        status?.blankSigningKey === true) && (
        <BrokenFormatWarning
          repoPath={repoPath}
          onFixed={refresh}
          // Both faults are "a blank value git treats as real", but they break
          // different things: a bad gpg.format stops every commit, a blank
          // signing key stops only signed ones. Saying which is what makes the
          // warning match what the user just saw fail.
          blankKeyOnly={status?.brokenFormat == null && status?.blankSigningKey === true}
        />
      )}

      {status?.missingSigningKey === true && (
        <MissingKeyWarning repoPath={repoPath} status={status} onFixed={refresh} />
      )}

      {status && status.keys.length === 0 ? (
        <CreateKeyForm repoPath={repoPath} onCreated={refresh} />
      ) : (
        status && <HasKey repoPath={repoPath} status={status} onChanged={refresh} />
      )}
    </Section>
  )
}

/**
 * Signing is on, but the key it names is not in the keyring.
 *
 * The worst version of this is silent: with no keys at all the panel used to
 * show "Set up signing", so a repository failing every commit looked like one
 * that had never been configured -- and the fix on offer (make a new key) was
 * not the fix needed. Says which scope turned signing on, because a repository
 * signing against a global default of off is exactly where people stop looking.
 */
function MissingKeyWarning({
  repoPath,
  status,
  onFixed,
}: {
  repoPath: string
  status: SigningStatus
  onFixed: () => void
}) {
  const [busy, setBusy] = useState(false)

  const turnOff = async () => {
    setBusy(true)
    const res = await commands.setSigningEnabled(repoPath, false, null)
    setBusy(false)
    if (res.status === 'ok') {
      toast.success('Signing is off. Your commits will go through now.')
      onFixed()
    } else {
      toast.error(res.error)
    }
  }

  const useInstead = async (key: SigningKey) => {
    setBusy(true)
    const res = await commands.setSigningEnabled(repoPath, true, key.id)
    setBusy(false)
    if (res.status === 'ok') {
      toast.success('Signing now uses that key.')
      onFixed()
    } else {
      toast.error(res.error)
    }
  }

  return (
    <div className="mb-4 rounded-md border border-amber-500/40 bg-amber-500/10 px-3 py-2.5">
      <div className="flex items-start gap-3">
        <AlertTriangle size={15} className="mt-0.5 flex-none text-amber-500" />
        <div className="min-w-0 flex-1">
          <div className="text-xs font-semibold text-foreground">
            Your commits are being signed with a key that no longer exists
          </div>
          <div className="mt-0.5 text-2xs text-muted-foreground">
            Signing is turned on
            {status.signingScope != null ? ` in ${status.signingScope}` : ''} and set to use{' '}
            <span className="font-mono text-foreground">{status.configuredKey}</span>, which is not
            on this computer. Every commit here will fail until you pick another key or turn
            signing off.
          </div>

          <div className="mt-2 flex flex-wrap items-center gap-2">
            {status.keys.map((k) => (
              <Button
                key={k.id}
                size="sm"
                variant="secondary"
                disabled={busy}
                onClick={() => void useInstead(k)}
              >
                <KeyRound size={12} />
                Use {k.uid || k.id}
              </Button>
            ))}
            <Button size="sm" variant="ghost" disabled={busy} onClick={() => void turnOff()}>
              Turn signing off
            </Button>
          </div>

          {status.keys.length === 0 && (
            <div className="mt-2 text-2xs text-muted-foreground">
              There are no keys on this computer to switch to. Turn signing off, or make a key
              below.
            </div>
          )}
        </div>
      </div>
    </div>
  )
}

/**
 * Two settings that are blank rather than absent, which git treats as real
 * values and then chokes on.
 *
 * A `gpg.format` git refuses to accept makes it reject *every* commit, signed
 * or not. A blank `user.signingkey` is handed straight to gpg, which answers
 * `skipped "": Invalid user ID` and fails only the signed ones. Both errors
 * name a config key most people have never heard of, and neither says which
 * scope holds it, so offer to clear them rather than leaving anyone stuck.
 */
function BrokenFormatWarning({
  repoPath,
  onFixed,
  blankKeyOnly,
}: {
  repoPath: string
  onFixed: () => void
  /** True when the only fault is an empty signing key, which breaks less. */
  blankKeyOnly: boolean
}) {
  const [fixing, setFixing] = useState(false)

  const fix = async () => {
    setFixing(true)
    const res = await commands.repairSigningFormat(repoPath)
    setFixing(false)
    if (res.status === 'ok') {
      toast.success(
        blankKeyOnly ? 'Fixed. Signing should work again.' : 'Fixed. Committing should work again.'
      )
      onFixed()
    } else {
      toast.error(res.error)
    }
  }

  return (
    <div className="mb-4 flex items-start gap-3 rounded-md border border-amber-500/40 bg-amber-500/10 px-3 py-2.5">
      <AlertTriangle size={15} className="mt-0.5 flex-none text-amber-500" />
      <div className="min-w-0 flex-1">
        <div className="text-xs font-semibold text-foreground">
          {blankKeyOnly
            ? 'A setting on this computer is blocking signed commits'
            : 'A setting on this computer is blocking every commit'}
        </div>
        <div className="mt-0.5 text-2xs text-muted-foreground">
          {blankKeyOnly
            ? 'Your git settings have an empty signing key. Git treats that as a real choice and hands it to gpg, which turns it down, so signed commits fail. Clearing it lets your key be picked normally.'
            : 'Your git settings have a signing format that git does not understand, so it refuses to make commits. Clearing it puts things back to normal.'}
        </div>
      </div>
      <Button size="sm" variant="secondary" className="h-7 flex-none" onClick={fix} disabled={fixing}>
        {fixing ? <Loader2 size={12} className="animate-spin" /> : 'Fix it'}
      </Button>
    </div>
  )
}

/**
 * Make a key and sign with it. Used both for the first key and for adding
 * another one later -- someone with a work key and a personal key needs to
 * make the second one from inside the app, not on the command line.
 */
function CreateKeyForm({
  repoPath,
  onCreated,
  onCancel,
}: {
  repoPath: string
  onCreated: () => void
  /** Shown as a Cancel button when this is an extra key, not the first. */
  onCancel?: () => void
}) {
  const { identity } = useGitIdentity()
  const [name, setName] = useState('')
  const [email, setEmail] = useState('')
  const [busy, setBusy] = useState(false)
  const [touched, setTouched] = useState(false)

  // Pre-fill from git, so the common case is one click rather than retyping
  // what git already knows. Skipped once the user edits a field, or their
  // typing would be overwritten when the identity finishes loading.
  useEffect(() => {
    if (identity && !touched) {
      setName(identity.name)
      setEmail(identity.email)
    }
  }, [identity, touched])

  const create = async () => {
    setBusy(true)
    const made = await commands.createSigningKey(name, email)
    if (made.status !== 'ok') {
      setBusy(false)
      toast.error(made.error)
      return
    }

    // Making a key the user then has to switch on themselves is a half-finished
    // action; turn signing on in the same step.
    const enabled = await commands.setSigningEnabled(repoPath, true, made.data)
    setBusy(false)
    if (enabled.status !== 'ok') {
      toast.error(enabled.error)
      return
    }
    toast.success('Your signing key is ready. New commits will be signed.')
    onCreated()
  }

  const ready = name.trim().length > 0 && email.trim().length > 0 && !busy

  return (
    <div className="rounded-md border border-border bg-panel p-4">
      <div className="flex items-center gap-2">
        <Sparkles size={15} className="text-accent-text" />
        <div className="text-xs font-semibold text-foreground">
          {onCancel ? 'Make another key' : 'Set up signing'}
        </div>
      </div>
      <p className="mt-1.5 text-2xs text-muted-foreground">
        GitWyrm can make a signing key for you. Use the same name and email you commit with here,
        so your host can match the key to you. Making a key also starts signing with it.
      </p>

      <div className="mt-3 grid gap-2 sm:grid-cols-2">
        <Input
          value={name}
          onChange={(e) => {
            setTouched(true)
            setName(e.target.value)
          }}
          placeholder="Your name"
          className="h-8 bg-background text-xs"
          aria-label="Your name"
        />
        <Input
          value={email}
          onChange={(e) => {
            setTouched(true)
            setEmail(e.target.value)
          }}
          placeholder="you@example.com"
          className="h-8 bg-background text-xs"
          aria-label="Your email"
        />
      </div>

      <div className="mt-3 flex items-center gap-2">
        <Button size="sm" className="h-8" onClick={create} disabled={!ready}>
          {busy ? (
            <>
              <Loader2 size={13} className="animate-spin" />
              Making your key...
            </>
          ) : (
            <>
              <KeyRound size={13} />
              Make a signing key
            </>
          )}
        </Button>
        {onCancel && (
          <Button size="sm" variant="ghost" className="h-8" onClick={onCancel} disabled={busy}>
            Cancel
          </Button>
        )}
      </div>
    </div>
  )
}

/** Pull the email out of a gpg uid ("Real Name <email>"). */
function uidEmail(uid: string): string {
  return uid.match(/<([^>]+)>/)?.[1]?.trim().toLowerCase() ?? ''
}

/** Keys exist: pick which one signs, toggle signing, and share the public half. */
function HasKey({
  repoPath,
  status,
  onChanged,
}: {
  repoPath: string
  status: SigningStatus
  onChanged: () => void
}) {
  const [busy, setBusy] = useState(false)
  const [making, setMaking] = useState(false)
  const [commitEmail, setCommitEmail] = useState('')

  // The email commits in THIS repository actually carry, which a profile
  // override or folder rule can make different from the global identity. The
  // warning below is only true if it compares against what git will really use.
  useEffect(() => {
    void (async () => {
      const res = await commands.getEffectiveIdentity(repoPath)
      setCommitEmail(res.status === 'ok' ? res.data.email : '')
    })()
  }, [repoPath, status])

  // Only a key git is actually configured with counts as active. Falling back
  // to keys[0] would silently sign with whichever key gpg happened to list
  // first -- a work key on a personal commit, with nothing on screen saying so.
  const active = status.keys.find((k) => k.id === status.configuredKey) ?? null

  // Signing with a key whose email is not the one on your commits gets you an
  // unverified badge on the host, which is confusing to debug. Say it plainly.
  const myEmail = commitEmail.trim().toLowerCase()
  const mismatch =
    active && myEmail && uidEmail(active.uid) && uidEmail(active.uid) !== myEmail
      ? uidEmail(active.uid)
      : null

  const apply = async (on: boolean, keyId: string | null) => {
    setBusy(true)
    const res = await commands.setSigningEnabled(repoPath, on, keyId)
    setBusy(false)
    if (res.status !== 'ok') {
      toast.error(res.error)
      return
    }
    toast.success(on ? 'New commits will be signed.' : 'Commits will no longer be signed.')
    onChanged()
  }

  const chooseKey = (keyId: string) => {
    if (!keyId) return
    // Picking a key is only meaningful if signing is on, so turn it on in the
    // same step rather than leaving a chosen-but-inactive key on screen.
    void apply(true, keyId)
  }

  if (making) {
    return (
      <CreateKeyForm repoPath={repoPath} onCreated={onChanged} onCancel={() => setMaking(false)} />
    )
  }

  return (
    <div className="grid gap-3">
      <SettingRow
        label="Sign my commits"
        searchId="signing-enabled"
        hint="Applies to the repository open in the active tab."
      >
        <div className="flex h-8 items-center gap-2">
          <label className="flex cursor-pointer items-center gap-2 text-xs text-foreground">
            <input
              type="checkbox"
              checked={status.signingEnabled}
              onChange={(e) => apply(e.target.checked, active?.id ?? status.keys[0]?.id ?? null)}
              disabled={busy}
              className="size-3.5 accent-[var(--gw-accent)]"
            />
            Sign every commit I make here
          </label>
          {busy && <Loader2 size={12} className="animate-spin text-muted-foreground" />}
          {!busy && status.signingEnabled && (
            <span className="flex items-center gap-1 text-2xs text-emerald-500">
              <ShieldCheck size={12} />
              On
            </span>
          )}
        </div>
      </SettingRow>

      <SettingRow
        label="Key to sign with"
        searchId="signing-key"
        hint="The key used for new commits in this repository."
      >
        <div className="grid gap-2">
          <div className="flex gap-2">
            <select
              className="h-8 w-full rounded-md border border-input bg-background px-2 text-xs text-foreground outline-none focus:border-ring"
              value={active?.id ?? ''}
              disabled={busy}
              aria-label="Key to sign with"
              onChange={(e) => chooseKey(e.target.value)}
            >
              {!active && <option value="">Pick a key…</option>}
              {status.keys.map((k) => (
                <option key={k.id} value={k.id}>
                  {k.uid || k.id}
                </option>
              ))}
            </select>
            <Button
              size="sm"
              variant="secondary"
              className="h-8 flex-none"
              disabled={busy}
              onClick={() => setMaking(true)}
            >
              <KeyRound size={12} />
              New key
            </Button>
          </div>

          {mismatch && (
            <div className="flex items-start gap-2 rounded-md border border-amber-500/40 bg-amber-500/10 px-2.5 py-2">
              <AlertTriangle size={13} className="mt-0.5 flex-none text-amber-500" />
              <div className="min-w-0 text-2xs leading-relaxed text-muted-foreground">
                This key belongs to{' '}
                <span className="font-medium text-foreground">{mismatch}</span>, but your commits
                here are made as{' '}
                <span className="font-medium text-foreground">{commitEmail}</span>. Your host will
                not show these commits as verified. Pick a key with the matching email, or make
                one.
              </div>
            </div>
          )}
        </div>
      </SettingRow>

      {active && <KeyCard keyInfo={active} repoPath={repoPath} onDeleted={onChanged} />}
    </div>
  )
}

function KeyCard({
  keyInfo,
  repoPath,
  onDeleted,
}: {
  keyInfo: SigningKey
  repoPath: string
  onDeleted: () => void
}) {
  const published = useWorkspaceStore((s) => s.signingKeysPublished)
  const markPublished = useWorkspaceStore((s) => s.markSigningKeyPublished)

  const [copied, setCopied] = useState(false)
  const [opened, setOpened] = useState(false)
  const [confirmDelete, setConfirmDelete] = useState(false)
  const [deleting, setDeleting] = useState(false)

  const isPublished = published.includes(keyInfo.fingerprint)

  const copyPublicKey = async () => {
    const res = await commands.exportSigningKey(keyInfo.id)
    if (res.status !== 'ok') {
      toast.error(res.error)
      return
    }
    await navigator.clipboard.writeText(res.data)
    // Stays ticked. An earlier version cleared this on a timer, which undid the
    // checkmark while the user was still on the host tab pasting it.
    setCopied(true)
    toast.success('Public key copied. Now paste it on your host.')
  }

  const openHostSettings = async () => {
    const { openUrl } = await import('@tauri-apps/plugin-opener')
    await openUrl('https://github.com/settings/gpg/new')
    setOpened(true)
  }

  const remove = async () => {
    setDeleting(true)
    const res = await commands.deleteSigningKey(repoPath, keyInfo.id, keyInfo.fingerprint)
    setDeleting(false)
    setConfirmDelete(false)
    if (res.status !== 'ok') {
      toast.error(res.error)
      return
    }
    toast.success('Key deleted.')
    onDeleted()
  }

  return (
    <div className="rounded-md border border-border bg-panel p-3">
      <div className="flex items-start gap-2">
        <KeyRound size={14} className="mt-0.5 flex-none text-accent-text" />
        <div className="min-w-0 flex-1">
          <div className="truncate text-xs font-semibold text-foreground">
            {keyInfo.uid || 'Your signing key'}
          </div>
          <div className="mt-0.5 font-mono text-2xs text-muted-foreground">
            {keyInfo.fingerprint}
          </div>
        </div>
        {isPublished && (
          <span className="flex flex-none items-center gap-1 text-2xs text-emerald-500">
            <ShieldCheck size={12} />
            On your host
          </span>
        )}
        <Button
          size="sm"
          variant="ghost"
          className="h-7 flex-none text-muted-foreground hover:text-removed"
          tooltip="Delete this key"
          onClick={() => setConfirmDelete(true)}
        >
          <Trash2 size={12} />
        </Button>
      </div>

      {/* Only while the key still needs publishing. The key alone signs nothing
          anyone can verify: until the public half is on the host, commits show
          as Unverified, which reads as a bug rather than an unfinished setup.
          Once the user says they have done it the panel goes quiet for good --
          keys that predate this setting count as done, so nobody is nagged
          about a key they set up years ago. */}
      {!isPublished && (
        <div className="mt-3 rounded-md border border-amber-500/40 bg-amber-500/10 p-2.5">
          <div className="flex items-center gap-1.5 text-xs font-semibold text-foreground">
            <AlertTriangle size={13} className="flex-none text-amber-500" />
            Two steps left, or your commits still show as unverified
          </div>
          <p className="mt-1 text-2xs leading-relaxed text-muted-foreground">
            Your commits are being signed already. For GitHub to show the Verified badge, it needs
            the public half of this key - GitWyrm cannot upload it for you.
          </p>

          <PublishSteps
            copied={copied}
            opened={opened}
            onCopy={copyPublicKey}
            onOpen={openHostSettings}
          />

          <Button
            size="sm"
            className="mt-3 h-7 w-full"
            onClick={() => {
              markPublished(keyInfo.fingerprint)
              toast.success('Nice. Your commits should show as Verified from now on.')
            }}
          >
            <Check size={12} />
            I have added it - stop reminding me
          </Button>
        </div>
      )}

      {isPublished && (
        <div className="mt-2.5 flex flex-wrap gap-2">
          <Button size="sm" variant="secondary" className="h-7" onClick={copyPublicKey}>
            {copied ? <Check size={12} className="text-emerald-500" /> : <Copy size={12} />}
            {copied ? 'Copied' : 'Copy public key'}
          </Button>
          <Button size="sm" variant="secondary" className="h-7" onClick={openHostSettings}>
            <ExternalLink size={12} />
            Open GitHub
          </Button>
        </div>
      )}

      <ConfirmDialog
        open={confirmDelete}
        onOpenChange={setConfirmDelete}
        title="Delete this signing key?"
        description="This cannot be undone. Commits you already signed stay valid, but nothing new can be signed with this key. If this repository is set to sign with it, signing is turned off."
        confirmLabel="Delete key"
        destructive
        pending={deleting}
        pendingLabel="Deleting..."
        keepOpenOnConfirm
        onConfirm={remove}
      />
    </div>
  )
}


/* -------------------------------------------------------------------- tools */

/** The git and gpg the app runs, and how to override either. */
function ToolsSection() {
  const gitExecutable = useWorkspaceStore((s) => s.gitExecutable)
  const setGitExecutable = useWorkspaceStore((s) => s.setGitExecutable)
  const gpgExecutable = useWorkspaceStore((s) => s.gpgExecutable)
  const setGpgExecutable = useWorkspaceStore((s) => s.setGpgExecutable)

  const gitStatus = useToolCheck(async (): Promise<ToolState> => {
    const res = await commands.gitToolInfo()
    if (res.status !== 'ok') return { state: 'error', message: res.error }
    if (!res.data.version) {
      return { state: 'error', message: 'No working git found at that path.' }
    }
    return { state: 'ok', version: res.data.version, source: res.data.source }
  }, [gitExecutable])

  const gpgStatus = useToolCheck(async (): Promise<ToolState> => {
    const res = await commands.gpgToolInfo()
    if (res.status !== 'ok') return { state: 'error', message: res.error }
    if (!res.data.version) {
      return { state: 'error', message: 'No working gpg found at that path.' }
    }
    return { state: 'ok', version: res.data.version, source: res.data.source }
  }, [gpgExecutable])

  const browse = async (title: string, apply: (path: string) => void) => {
    const { open: openDialog } = await import('@tauri-apps/plugin-dialog')
    const picked = await openDialog({
      title,
      multiple: false,
      directory: false,
      filters: [{ name: 'Programs', extensions: ['exe'] }],
    })
    if (typeof picked === 'string') apply(picked)
  }

  return (
    <Section
      title="Programs GitWyrm uses"
      blurb="Choose a specific Git or GPG program, or let GitWyrm decide."
    >
      <SettingRow
        label="Git"
        searchId="git-executable"
        hint="Used to fetch, pull, push, and clone. Leave blank to let GitWyrm decide."
      >
        <ToolStatusRow
          value={gitExecutable}
          onCommit={setGitExecutable}
          onBrowse={() => browse('Select the git program', setGitExecutable)}
          status={gitStatus}
          browseLabel={`Browse for ${executableName('git')}`}
        />
      </SettingRow>

      <SettingRow
        label="GPG"
        searchId="gpg-executable"
        hint="Used to sign commits. Leave blank to let GitWyrm decide."
      >
        <ToolStatusRow
          value={gpgExecutable}
          onCommit={setGpgExecutable}
          onBrowse={() => browse('Select the gpg program', setGpgExecutable)}
          status={gpgStatus}
          browseLabel={`Browse for ${executableName('gpg')}`}
        />
      </SettingRow>
    </Section>
  )
}

/* ------------------------------------------------------------------ shared */

function Section({
  title,
  blurb,
  children,
}: {
  title: string
  blurb: string
  children: React.ReactNode
}) {
  return (
    <section>
      <h3 className="text-xs font-semibold text-foreground">{title}</h3>
      <p className="mb-3 mt-0.5 max-w-prose text-2xs leading-relaxed text-muted-foreground">{blurb}</p>
      {children}
    </section>
  )
}
