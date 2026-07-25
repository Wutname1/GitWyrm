import { useCallback, useState } from 'react'
import {
  AlertTriangle,
  Check,
  Copy,
  ExternalLink,
  KeyRound,
  Loader2,
  ShieldCheck,
  Sparkles,
} from 'lucide-react'
import { toast } from 'sonner'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { commands, type SigningKey, type SigningStatus } from '@/lib/bindings'
import { useActiveRepo, useWorkspaceStore } from '@/stores/workspaceStore'
import { SettingRow } from './SettingRow'
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
      <ToolsSection />
    </div>
  )
}

/* ------------------------------------------------------------------ signing */

function SigningSection({ repoPath }: { repoPath: string | null }) {
  // Bumped after any change that alters signing state, to re-read it.
  const [nonce, setNonce] = useState(0)
  const refresh = useCallback(() => setNonce((n) => n + 1), [])

  const [status, setStatus] = useState<SigningStatus | null>(null)
  const [loading, setLoading] = useState(true)

  useToolCheck(async (): Promise<ToolState> => {
    if (!repoPath) {
      setLoading(false)
      return { state: 'checking' }
    }
    setLoading(true)
    const res = await commands.getSigningStatus(repoPath)
    setLoading(false)
    if (res.status === 'ok') {
      setStatus(res.data)
      return { state: 'ok', version: '', source: res.data.gpgSource }
    }
    setStatus(null)
    return { state: 'error', message: res.error }
  }, [repoPath, nonce])

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
      {status?.brokenFormat !== null && status?.brokenFormat !== undefined && (
        <BrokenFormatWarning repoPath={repoPath} onFixed={refresh} />
      )}

      {status && status.keys.length === 0 ? (
        <NoKeyYet repoPath={repoPath} onCreated={refresh} />
      ) : (
        status && <HasKey repoPath={repoPath} status={status} onChanged={refresh} />
      )}
    </Section>
  )
}

/**
 * A `gpg.format` git refuses to accept makes it reject *every* commit, signed
 * or not, with an error that names a config key most people have never heard
 * of. Offer to clear it rather than leaving them stuck.
 */
function BrokenFormatWarning({ repoPath, onFixed }: { repoPath: string; onFixed: () => void }) {
  const [fixing, setFixing] = useState(false)

  const fix = async () => {
    setFixing(true)
    const res = await commands.repairSigningFormat(repoPath)
    setFixing(false)
    if (res.status === 'ok') {
      toast.success('Fixed. Committing should work again.')
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
          A setting on this computer is blocking every commit
        </div>
        <div className="mt-0.5 text-2xs text-muted-foreground">
          Your git settings have a signing format that git does not understand, so it refuses to
          make commits. Clearing it puts things back to normal.
        </div>
      </div>
      <Button size="sm" variant="secondary" className="h-7 flex-none" onClick={fix} disabled={fixing}>
        {fixing ? <Loader2 size={12} className="animate-spin" /> : 'Fix it'}
      </Button>
    </div>
  )
}

/** The one-click path: no key exists, so offer to make one. */
function NoKeyYet({ repoPath, onCreated }: { repoPath: string; onCreated: () => void }) {
  const [name, setName] = useState('')
  const [email, setEmail] = useState('')
  const [busy, setBusy] = useState(false)

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
        <div className="text-xs font-semibold text-foreground">Set up signing</div>
      </div>
      <p className="mt-1.5 text-2xs text-muted-foreground">
        GitWyrm can make a signing key for you. Use the same name and email you commit with, so
        your host can match the key to you.
      </p>

      <div className="mt-3 grid gap-2 sm:grid-cols-2">
        <Input
          value={name}
          onChange={(e) => setName(e.target.value)}
          placeholder="Your name"
          className="h-8 bg-background text-xs"
          aria-label="Your name"
        />
        <Input
          value={email}
          onChange={(e) => setEmail(e.target.value)}
          placeholder="you@example.com"
          className="h-8 bg-background text-xs"
          aria-label="Your email"
        />
      </div>

      <Button size="sm" className="mt-3 h-8" onClick={create} disabled={!ready}>
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
    </div>
  )
}

/** A key exists: show it, let signing be toggled, and help share the public half. */
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
  const active =
    status.keys.find((k) => k.id === status.configuredKey) ?? status.keys[0] ?? null

  const toggle = async (on: boolean) => {
    setBusy(true)
    const res = await commands.setSigningEnabled(repoPath, on, active?.id ?? null)
    setBusy(false)
    if (res.status === 'ok') {
      toast.success(on ? 'New commits will be signed.' : 'Commits will no longer be signed.')
      onChanged()
    } else {
      toast.error(res.error)
    }
  }

  return (
    <div className="grid gap-3">
      <SettingRow
        label="Sign my commits"
        hint="Applies to the repository open in the active tab."
      >
        <div className="flex h-8 items-center gap-2">
          <label className="flex cursor-pointer items-center gap-2 text-xs text-foreground">
            <input
              type="checkbox"
              checked={status.signingEnabled}
              onChange={(e) => toggle(e.target.checked)}
              disabled={busy || !active}
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

      {active && <KeyCard keyInfo={active} />}
    </div>
  )
}

function KeyCard({ keyInfo }: { keyInfo: SigningKey }) {
  const [copied, setCopied] = useState(false)

  const copyPublicKey = async () => {
    const res = await commands.exportSigningKey(keyInfo.id)
    if (res.status !== 'ok') {
      toast.error(res.error)
      return
    }
    await navigator.clipboard.writeText(res.data)
    // Rule #1: the button itself confirms, not just a toast that may be missed.
    setCopied(true)
    setTimeout(() => setCopied(false), 2000)
    toast.success('Public key copied. Paste it on your host to finish.')
  }

  const openHostSettings = async () => {
    const { openUrl } = await import('@tauri-apps/plugin-opener')
    await openUrl('https://github.com/settings/gpg/new')
  }

  return (
    <div className="rounded-md border border-border bg-panel p-3">
      <div className="flex items-center gap-2">
        <KeyRound size={14} className="text-accent-text" />
        <span className="truncate text-xs font-semibold text-foreground">
          {keyInfo.uid || 'Your signing key'}
        </span>
      </div>
      <div className="mt-1 font-mono text-2xs text-muted-foreground">{keyInfo.fingerprint}</div>

      <p className="mt-2.5 text-2xs text-muted-foreground">
        To get the Verified badge, your host needs the public half of this key. Copy it, then add
        it in your account settings.
      </p>

      <div className="mt-2.5 flex flex-wrap gap-2">
        <Button size="sm" variant="secondary" className="h-7" onClick={copyPublicKey}>
          {copied ? <Check size={12} className="text-emerald-500" /> : <Copy size={12} />}
          {copied ? 'Copied' : 'Copy public key'}
        </Button>
        <Button size="sm" variant="secondary" className="h-7" onClick={openHostSettings}>
          <ExternalLink size={12} />
          Add it on GitHub
        </Button>
      </div>
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
      blurb="GitWyrm comes with its own copies of git and gpg, so it works on a computer that has neither. If you already have your own, GitWyrm uses those instead."
    >
      <SettingRow
        label="Git"
        hint="Used to fetch, pull, push, and clone. Leave blank to let GitWyrm decide."
      >
        <ToolStatusRow
          value={gitExecutable}
          onCommit={setGitExecutable}
          onBrowse={() => browse('Select the git program', setGitExecutable)}
          status={gitStatus}
          browseLabel="Browse for git.exe"
        />
      </SettingRow>

      <SettingRow
        label="GPG"
        hint="Used to sign commits. Leave blank to let GitWyrm decide."
      >
        <ToolStatusRow
          value={gpgExecutable}
          onCommit={setGpgExecutable}
          onBrowse={() => browse('Select the gpg program', setGpgExecutable)}
          status={gpgStatus}
          browseLabel="Browse for gpg.exe"
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
      <h3 className="text-xs font-bold uppercase tracking-[.06em] text-sub">{title}</h3>
      <p className="mb-3 mt-1 max-w-prose text-2xs text-muted-foreground">{blurb}</p>
      {children}
    </section>
  )
}
