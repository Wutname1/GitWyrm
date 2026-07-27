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
import { commands, type SigningStatus, type SshKey } from '@/lib/bindings'
import { useGitIdentity } from '@/lib/useGitIdentity'
import { useWorkspaceStore } from '@/stores/workspaceStore'
import { SettingRow } from './SettingRow'
import { PublishSteps } from './PublishSteps'

/**
 * Sign commits with an SSH key.
 *
 * The appeal over GPG is that most people already have a key in `~/.ssh`, so
 * setup is "use the one I have" rather than making anything new. Git has
 * supported this since 2.34 and every major host accepts it.
 */
export function SshSigning({
  repoPath,
  status,
  onChanged,
}: {
  repoPath: string
  status: SigningStatus
  onChanged: () => void
}) {
  const { identity } = useGitIdentity()
  const [keys, setKeys] = useState<SshKey[] | null>(null)
  const [busy, setBusy] = useState(false)
  const [making, setMaking] = useState(false)

  const loadKeys = useCallback(async () => {
    const res = await commands.listSshKeys()
    if (res.status === 'ok') {
      setKeys(res.data)
    } else {
      setKeys([])
      toast.error(res.error)
    }
  }, [])

  useEffect(() => {
    void loadKeys()
  }, [loadKeys])

  const activePath = status.sshKeyPath ?? null
  const active = keys?.find((k) => k.path === activePath) ?? null

  const use = async (path: string) => {
    const email = identity?.email.trim()
    if (!email) {
      toast.error('Add your email in Settings > General first, so the key can be matched to you.')
      return
    }
    setBusy(true)
    const res = await commands.enableSshSigning(repoPath, path, email)
    setBusy(false)
    if (res.status !== 'ok') {
      toast.error(res.error)
      return
    }
    toast.success('Signing with that key from now on.')
    onChanged()
  }

  if (keys === null) {
    return (
      <div className="flex items-center gap-2 px-1 py-4 text-xs text-muted-foreground">
        <Loader2 size={14} className="animate-spin" />
        Looking for your SSH keys...
      </div>
    )
  }

  return (
    <div className="grid gap-3">
      <SettingRow
        label="Key to sign with"
        searchId="ssh-signing-key"
        hint="Any SSH key on this computer can sign. Most people use the one they already push with."
      >
        <div className="grid gap-2">
          <div className="flex gap-2">
            <select
              className="h-8 w-full rounded-md border border-input bg-background px-2 text-xs text-foreground outline-none focus:border-ring"
              value={activePath ?? ''}
              disabled={busy || keys.length === 0}
              aria-label="SSH key to sign with"
              onChange={(e) => use(e.target.value)}
            >
              {!active && <option value="">Pick a key...</option>}
              {keys.map((k) => (
                <option key={k.path} value={k.path}>
                  {k.comment ? `${k.name} - ${k.comment}` : k.name}
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
          {keys.length === 0 && (
            <p className="text-2xs text-muted-foreground">
              No SSH keys found on this computer. Make one to start signing.
            </p>
          )}
        </div>
      </SettingRow>

      {making && (
        <CreateSshKeyForm
          onCancel={() => setMaking(false)}
          onCreated={async (key) => {
            setMaking(false)
            await loadKeys()
            // Making a key the user then has to select themselves is a
            // half-finished action; start signing with it right away.
            await use(key.path)
          }}
        />
      )}

      {active && (
        <SshKeyCard keyInfo={active} onDeleted={onChanged} onReload={loadKeys} />
      )}
    </div>
  )
}

/** Make a new SSH key, named so the user can tell their keys apart later. */
function CreateSshKeyForm({
  onCreated,
  onCancel,
}: {
  onCreated: (key: SshKey) => void
  onCancel: () => void
}) {
  const { identity } = useGitIdentity()
  const [name, setName] = useState('gitwyrm_signing')
  const [busy, setBusy] = useState(false)

  const create = async () => {
    const email = identity?.email.trim()
    if (!email) {
      toast.error('Add your email in Settings > General first.')
      return
    }
    setBusy(true)
    const res = await commands.createSshKey(name, email)
    setBusy(false)
    if (res.status !== 'ok') {
      toast.error(res.error)
      return
    }
    toast.success('SSH key made.')
    onCreated(res.data)
  }

  return (
    <div className="rounded-md border border-border bg-panel p-4">
      <div className="flex items-center gap-2">
        <Sparkles size={15} className="text-accent-text" />
        <div className="text-xs font-semibold text-foreground">Make an SSH key</div>
      </div>
      <p className="mt-1.5 text-2xs text-muted-foreground">
        Saved in your .ssh folder and labelled with your email. Give it a name you will recognise
        later.
      </p>

      <Input
        value={name}
        onChange={(e) => setName(e.target.value)}
        onKeyDown={(e) => e.key === 'Enter' && create()}
        placeholder="gitwyrm_signing"
        className="mt-3 h-8 bg-background font-mono text-xs"
        aria-label="Name for this key"
      />

      <div className="mt-3 flex gap-2">
        <Button size="sm" className="h-8" onClick={create} disabled={busy}>
          {busy ? (
            <>
              <Loader2 size={13} className="animate-spin" />
              Making your key...
            </>
          ) : (
            <>
              <KeyRound size={13} />
              Make key
            </>
          )}
        </Button>
        <Button size="sm" variant="ghost" className="h-8" onClick={onCancel} disabled={busy}>
          Cancel
        </Button>
      </div>
    </div>
  )
}

/** The chosen SSH key: publish checklist, copy, and delete. */
function SshKeyCard({
  keyInfo,
  onDeleted,
  onReload,
}: {
  keyInfo: SshKey
  onDeleted: () => void
  onReload: () => Promise<void>
}) {
  const published = useWorkspaceStore((s) => s.signingKeysPublished)
  const markPublished = useWorkspaceStore((s) => s.markSigningKeyPublished)

  const [copied, setCopied] = useState(false)
  const [opened, setOpened] = useState(false)
  const [confirmDelete, setConfirmDelete] = useState(false)
  const [deleting, setDeleting] = useState(false)

  const isPublished = published.includes(keyInfo.fingerprint)

  const copyPublicKey = async () => {
    const res = await commands.readSshPublicKey(keyInfo.path)
    if (res.status !== 'ok') {
      toast.error(res.error)
      return
    }
    await navigator.clipboard.writeText(res.data)
    setCopied(true)
    toast.success('Public key copied. Now paste it on your host.')
  }

  const openHostSettings = async () => {
    const { openUrl } = await import('@tauri-apps/plugin-opener')
    // The signing-key form specifically. GitHub keeps signing keys and
    // authentication keys apart, and one added for pushing does not verify
    // commits - which is the single easiest way to get this wrong.
    await openUrl('https://github.com/settings/ssh/new')
    setOpened(true)
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
    await onReload()
    onDeleted()
  }

  return (
    <div className="rounded-md border border-border bg-panel p-3">
      <div className="flex items-start gap-2">
        <KeyRound size={14} className="mt-0.5 flex-none text-accent-text" />
        <div className="min-w-0 flex-1">
          <div className="truncate text-xs font-semibold text-foreground">
            {keyInfo.name}
            {keyInfo.algorithm && (
              <span className="ml-1.5 font-normal text-muted-foreground">{keyInfo.algorithm}</span>
            )}
          </div>
          <div className="mt-0.5 truncate font-mono text-2xs text-muted-foreground">
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

      {!isPublished ? (
        <div className="mt-3 rounded-md border border-amber-500/40 bg-amber-500/10 p-2.5">
          <div className="flex items-center gap-1.5 text-xs font-semibold text-foreground">
            <AlertTriangle size={13} className="flex-none text-amber-500" />
            Two steps left, or your commits still show as unverified
          </div>
          <p className="mt-1 text-2xs leading-relaxed text-muted-foreground">
            Add this key to your host as a <span className="font-semibold">signing</span> key. A key
            added for pushing does not count - GitHub keeps the two apart.
          </p>

          <PublishSteps
            copied={copied}
            opened={opened}
            onCopy={copyPublicKey}
            onOpen={openHostSettings}
            openLabel="Add it on GitHub as a signing key"
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
      ) : (
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
        title="Delete this SSH key?"
        description="This deletes the key files from your computer and cannot be undone. If you also use this key to push, that will stop working too. Commits you already signed stay valid."
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
