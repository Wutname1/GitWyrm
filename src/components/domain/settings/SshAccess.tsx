import { useCallback, useEffect, useState } from 'react'
import { useQueryClient } from '@tanstack/react-query'
import {
  AlertTriangle,
  Check,
  Copy,
  ExternalLink,
  KeyRound,
  Loader2,
  Plug,
  ShieldCheck,
  Sparkles,
  Trash2,
  X,
} from 'lucide-react'
import { toast } from 'sonner'
import { ConfirmDialog } from '@/components/modals/ConfirmDialog'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { commands, type SshKey, type SshKeyPairing, type SshTestResult } from '@/lib/bindings'
import { useGitIdentity } from '@/lib/useGitIdentity'
import { githubKeys, useGithubAuth, useGithubSshKeys } from '@/hooks/useGithub'
import { SettingRow } from './SettingRow'

/** Hosts worth offering by default. Anything else can be typed in. */
const COMMON_HOSTS = ['github.com', 'gitlab.com', 'bitbucket.org']

/** Where each host keeps its "add an SSH key" page. */
const HOST_KEY_PAGES: Record<string, string> = {
  'github.com': 'https://github.com/settings/ssh/new',
  'gitlab.com': 'https://gitlab.com/-/user_settings/ssh_keys',
  'bitbucket.org': 'https://bitbucket.org/account/settings/ssh-keys/',
}

/**
 * Connecting to a git host over SSH.
 *
 * The problem this solves is not "make a key" - it is that when SSH fails, git
 * only ever says `Permission denied (publickey)`. It never says which key it
 * offered, and never that a different key already on the machine would have
 * worked. Testing a host answers both.
 */
export function SshAccess() {
  const [host, setHost] = useState('github.com')
  const [keys, setKeys] = useState<SshKey[] | null>(null)
  const [configuredKey, setConfiguredKey] = useState<string | null>(null)
  const [result, setResult] = useState<SshTestResult | null>(null)
  const [testing, setTesting] = useState(false)
  const [making, setMaking] = useState(false)

  // Cross-referencing only works for github.com, and only once connected.
  const { data: githubLogin } = useGithubAuth()
  const isGithub = host.trim().toLowerCase() === 'github.com'
  const { data: pairings } = useGithubSshKeys(Boolean(githubLogin) && isGithub)
  const pairingFor = (fingerprint: string) =>
    pairings?.find((p) => p.fingerprint === fingerprint && p.localPath !== null)
  const githubOnly = pairings?.filter((p) => p.localPath === null) ?? []

  const loadKeys = useCallback(async () => {
    const res = await commands.listSshKeys()
    if (res.status === 'ok') setKeys(res.data)
    else {
      setKeys([])
      toast.error(res.error)
    }
  }, [])

  const loadConfigured = useCallback(async (forHost: string) => {
    const res = await commands.sshKeyForHost(forHost)
    setConfiguredKey(res.status === 'ok' ? res.data : null)
  }, [])

  useEffect(() => {
    void loadKeys()
  }, [loadKeys])

  useEffect(() => {
    setResult(null)
    void loadConfigured(host)
  }, [host, loadConfigured])

  const test = async () => {
    setTesting(true)
    setResult(null)
    const res = await commands.testSshHost(host)
    setTesting(false)
    if (res.status !== 'ok') {
      toast.error(res.error)
      return
    }
    setResult(res.data)
  }

  const useKey = async (keyPath: string) => {
    // A plain timestamp for the backup filename. Generated here rather than in
    // Rust so the name matches the user's clock, not UTC.
    const stamp = new Date().toISOString().replace(/[:.]/g, '-')
    const res = await commands.setSshKeyForHost(host, keyPath, stamp)
    if (res.status !== 'ok') {
      toast.error(res.error)
      return
    }
    toast.success(`${host} will use that key from now on. Your old config was backed up.`)
    await loadConfigured(host)
    setResult(null)
  }

  return (
    <div className="grid gap-3">
      <SettingRow
        label="Host"
        searchId="ssh-host"
        hint="Which git host to check. Testing tells you whether it accepts a key from this computer."
      >
        <div className="flex gap-2">
          <Input
            value={host}
            onChange={(e) => setHost(e.target.value)}
            onKeyDown={(e) => e.key === 'Enter' && test()}
            list="gitwyrm-ssh-hosts"
            placeholder="github.com"
            className="h-8 bg-background font-mono text-xs"
            aria-label="Host to test"
          />
          <datalist id="gitwyrm-ssh-hosts">
            {COMMON_HOSTS.map((h) => (
              <option key={h} value={h} />
            ))}
          </datalist>
          <Button
            size="sm"
            className="h-8 flex-none"
            onClick={test}
            disabled={testing || host.trim().length === 0}
          >
            {testing ? <Loader2 size={13} className="animate-spin" /> : <Plug size={13} />}
            {testing ? 'Testing...' : 'Test'}
          </Button>
        </div>
      </SettingRow>

      {configuredKey && (
        <div className="px-1 text-2xs text-muted-foreground">
          Right now {host} uses{' '}
          <span className="font-mono font-medium text-emerald-500">{configuredKey}</span>.
        </div>
      )}

      {result && <TestResult result={result} onUseKey={useKey} />}

      <SettingRow
        label="Your keys"
        searchId="ssh-keys"
        hint="Keys in your .ssh folder. Add one to a host's account settings to let it recognise you."
      >
        <div className="grid gap-2">
          {keys === null ? (
            <div className="flex items-center gap-1.5 text-2xs text-muted-foreground">
              <Loader2 size={12} className="animate-spin" />
              Looking for your keys...
            </div>
          ) : keys.length === 0 ? (
            <p className="text-2xs text-muted-foreground">
              No SSH keys on this computer yet.
            </p>
          ) : (
            keys.map((k) => (
              <KeyRow
                key={k.path}
                keyInfo={k}
                host={host}
                inUse={configuredKey === k.path}
                pairing={pairingFor(k.fingerprint)}
                onUse={() => useKey(k.path)}
                onDeleted={loadKeys}
              />
            ))
          )}

          <GithubOnlyKeys pairings={githubOnly} />

          {making ? (
            <CreateKeyForm
              onCancel={() => setMaking(false)}
              onCreated={async (key) => {
                setMaking(false)
                await loadKeys()
                await useKey(key.path)
              }}
            />
          ) : (
            <Button
              size="sm"
              variant="secondary"
              className="h-7 w-fit"
              onClick={() => setMaking(true)}
            >
              <KeyRound size={12} />
              Make a new key
            </Button>
          )}
        </div>
      </SettingRow>
    </div>
  )
}

/** The outcome of a connection test, said plainly. */
function TestResult({
  result,
  onUseKey,
}: {
  result: SshTestResult
  onUseKey: (path: string) => void
}) {
  if (result.ok) {
    return (
      <div className="flex items-start gap-2 rounded-md border border-emerald-500/40 bg-emerald-500/10 px-3 py-2.5">
        <ShieldCheck size={15} className="mt-0.5 flex-none text-emerald-500" />
        <div className="min-w-0 flex-1 text-xs text-foreground">{result.message}</div>
      </div>
    )
  }

  return (
    <div className="rounded-md border border-amber-500/40 bg-amber-500/10 px-3 py-2.5">
      <div className="flex items-start gap-2">
        <AlertTriangle size={15} className="mt-0.5 flex-none text-amber-500" />
        <div className="min-w-0 flex-1 text-xs text-foreground">{result.message}</div>
      </div>
      {result.workingKey && (
        <Button
          size="sm"
          className="mt-2.5 h-7"
          onClick={() => onUseKey(result.workingKey as string)}
        >
          <Check size={12} />
          Use that key for this host
        </Button>
      )}
    </div>
  )
}

/** One key: what it is, whether this host uses it, copy, and delete. */
function KeyRow({
  keyInfo,
  host,
  inUse,
  pairing,
  onUse,
  onDeleted,
}: {
  keyInfo: SshKey
  host: string
  inUse: boolean
  pairing: SshKeyPairing | undefined
  onUse: () => void
  onDeleted: () => Promise<void>
}) {
  const [copied, setCopied] = useState(false)
  const [confirmDelete, setConfirmDelete] = useState(false)
  const [deleting, setDeleting] = useState(false)
  const qc = useQueryClient()

  const copy = async () => {
    const res = await commands.readSshPublicKey(keyInfo.path)
    if (res.status !== 'ok') {
      toast.error(res.error)
      return
    }
    await navigator.clipboard.writeText(res.data)
    setCopied(true)
    toast.success('Public key copied. Paste it into your account settings on the host.')
  }

  const openHostPage = async () => {
    const { openUrl } = await import('@tauri-apps/plugin-opener')
    await openUrl(HOST_KEY_PAGES[host] ?? `https://${host}`)
    // They are being sent off to add this key, so the cached "not on GitHub"
    // answer is about to be wrong. Drop it now rather than showing a stale
    // badge for the rest of the cache window.
    qc.invalidateQueries({ queryKey: githubKeys.sshKeys })
  }

  const remove = async () => {
    setDeleting(true)
    const res = await commands.deleteSshKey(keyInfo.path)
    setDeleting(false)
    setConfirmDelete(false)
    if (res.status !== 'ok') {
      toast.error(res.error)
      return
    }
    toast.success('Key deleted.')
    await onDeleted()
  }

  return (
    <div className="rounded-md border border-border bg-panel p-2.5">
      <div className="flex items-start gap-2">
        <KeyRound size={13} className="mt-0.5 flex-none text-accent-text" />
        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-1.5">
            <span className="truncate text-xs font-medium text-foreground">{keyInfo.name}</span>
            {keyInfo.algorithm && (
              <span className="flex-none text-2xs text-muted-foreground">{keyInfo.algorithm}</span>
            )}
            <GithubBadge pairing={pairing} />
          </div>
          {/* GitHub's title is often the only real name a key has, so it beats
              the file comment when both exist. */}
          {pairing?.githubTitle ? (
            <div className="truncate text-2xs text-muted-foreground">
              "{pairing.githubTitle}" on GitHub
              {pairing.lastUsed ? ` - used ${pairing.lastUsed}` : ' - never used'}
            </div>
          ) : (
            keyInfo.comment && (
              <div className="truncate text-2xs text-muted-foreground">{keyInfo.comment}</div>
            )
          )}
        </div>
        <Button
          size="sm"
          variant="ghost"
          className="h-6 flex-none text-muted-foreground hover:text-removed"
          tooltip="Delete this key"
          onClick={() => setConfirmDelete(true)}
        >
          <Trash2 size={11} />
        </Button>
      </div>

      <div className="mt-2 flex flex-wrap gap-1.5">
        {inUse ? (
          <Button
            size="sm"
            variant="secondary"
            className="h-6 border-emerald-500/40 text-emerald-500 disabled:opacity-100"
            disabled
          >
            <Check size={11} />
            Used for {host}
          </Button>
        ) : (
          <Button size="sm" variant="secondary" className="h-6" onClick={onUse}>
            Use for {host}
          </Button>
        )}
        <Button size="sm" variant="secondary" className="h-6" onClick={copy}>
          {copied ? <Check size={11} className="text-emerald-500" /> : <Copy size={11} />}
          {copied ? 'Copied' : 'Copy public key'}
        </Button>
        <Button size="sm" variant="secondary" className="h-6" onClick={openHostPage}>
          <ExternalLink size={11} />
          Add on {host}
        </Button>
      </div>

      <ConfirmDialog
        open={confirmDelete}
        onOpenChange={setConfirmDelete}
        title="Delete this SSH key?"
        description="This deletes the key files from your computer and cannot be undone. Anything using this key to connect will stop working."
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

/**
 * Whether GitHub recognises this key, shown beside the key's name.
 *
 * Only rendered for github.com, and only when GitHub is connected -- there is
 * no equivalent for GitLab or Bitbucket, and a blank badge would read as "not
 * added" rather than "not checked".
 */
function GithubBadge({ pairing }: { pairing: SshKeyPairing | undefined }) {
  if (!pairing) return null

  if (pairing.githubTitle) {
    return (
      <span
        className="flex flex-none items-center gap-1 text-2xs text-emerald-500"
        title={
          pairing.lastUsed
            ? `On GitHub as "${pairing.githubTitle}". Last used ${pairing.lastUsed}.`
            : `On GitHub as "${pairing.githubTitle}". GitHub has never seen it used.`
        }
      >
        <Check size={10} />
        On GitHub
      </span>
    )
  }

  return (
    <span
      className="flex-none text-2xs text-amber-500"
      title="GitHub does not have this key, so it cannot recognise you with it."
    >
      Not on GitHub
    </span>
  )
}

/**
 * Keys GitHub has that this computer does not.
 *
 * Almost always a key added from another machine, which is normal -- so this
 * is worded as information, never as something to fix. It exists because
 * without it, a key you remember adding simply appears to have vanished.
 */
function GithubOnlyKeys({ pairings }: { pairings: SshKeyPairing[] }) {
  if (pairings.length === 0) return null

  return (
    <div className="rounded-md border border-dashed border-border bg-panel/50 p-2.5">
      <div className="text-2xs font-medium text-foreground">
        Also on your GitHub account
      </div>
      <p className="mt-0.5 text-2xs text-muted-foreground">
        {pairings.length === 1 ? 'This key was' : 'These keys were'} added from another computer, so
        the private half is not on this one. That is normal.
      </p>
      <div className="mt-2 grid gap-1">
        {pairings.map((p) => (
          <div key={p.fingerprint} className="flex items-center gap-1.5">
            <KeyRound size={11} className="flex-none text-muted-foreground" />
            <span className="truncate text-2xs text-foreground">{p.githubTitle}</span>
            <span className="flex-none text-2xs text-muted-foreground">
              {p.lastUsed ? `used ${p.lastUsed}` : 'never used'}
            </span>
          </div>
        ))}
      </div>
    </div>
  )
}

/** Make a new key, labelled with the user's email so hosts can match it. */
function CreateKeyForm({
  onCreated,
  onCancel,
}: {
  onCreated: (key: SshKey) => void
  onCancel: () => void
}) {
  const { identity } = useGitIdentity()
  const [name, setName] = useState('gitwyrm')
  const [busy, setBusy] = useState(false)

  const create = async () => {
    const email = identity?.email.trim()
    if (!email) {
      toast.error('Add your email in Settings > General first, so the key can be labelled.')
      return
    }
    setBusy(true)
    const res = await commands.createSshKey(name, email)
    setBusy(false)
    if (res.status !== 'ok') {
      toast.error(res.error)
      return
    }
    toast.success('Key made. Copy it and add it to your account on the host.')
    onCreated(res.data)
  }

  return (
    <div className="rounded-md border border-border bg-panel p-3">
      <div className="flex items-center gap-2">
        <Sparkles size={14} className="text-accent-text" />
        <div className="text-xs font-semibold text-foreground">Make a new key</div>
      </div>
      <p className="mt-1 text-2xs text-muted-foreground">
        Saved in your .ssh folder and labelled with your email. Give it a name you will recognise.
      </p>
      <div className="mt-2.5 flex gap-2">
        <Input
          value={name}
          onChange={(e) => setName(e.target.value)}
          onKeyDown={(e) => e.key === 'Enter' && create()}
          placeholder="gitwyrm"
          className="h-8 bg-background font-mono text-xs"
          aria-label="Name for this key"
        />
        <Button size="sm" className="h-8 flex-none" onClick={create} disabled={busy}>
          {busy ? <Loader2 size={13} className="animate-spin" /> : <KeyRound size={13} />}
          {busy ? 'Making...' : 'Make key'}
        </Button>
        <Button
          size="sm"
          variant="ghost"
          className="h-8 flex-none"
          onClick={onCancel}
          disabled={busy}
        >
          <X size={13} />
        </Button>
      </div>
    </div>
  )
}
