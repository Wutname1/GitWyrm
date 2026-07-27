import { useEffect, useMemo, useState } from 'react'
import { Check, Folder, KeyRound, Pencil, Plus, Trash2, UserRound } from 'lucide-react'
import { toast } from 'sonner'
import { ConfirmDialog } from '@/components/modals/ConfirmDialog'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { useProfiles, normalizeProfile, type LoadedProfile } from '@/hooks/useProfiles'
import {
  commands,
  type SigningKey,
  type SigningMethod,
  type SshKey,
} from '@/lib/bindings'
import { cn } from '@/lib/utils'
import { useActiveRepo } from '@/stores/workspaceStore'
import { SettingRow } from './SettingRow'

const selectClass =
  'h-8 w-full rounded-md border border-input bg-background px-2 text-xs text-foreground outline-none focus:border-ring'

/** Stable-enough id for a new profile; ids only need to be unique locally. */
function newId() {
  return `p${Date.now().toString(36)}`
}

function emptyProfile(): LoadedProfile {
  return {
    id: newId(),
    label: '',
    name: '',
    email: '',
    signing: { kind: 'none' },
    signCommits: false,
    folders: [],
  }
}

/**
 * Profiles: who you commit as, and the key you sign with, kept together.
 *
 * Setting a name, an email, and a signing key separately is how you end up
 * committing to a personal repo under a work address, or signing with a key
 * that does not match the commit. One switch moves all three (Rule #2).
 */
export function ProfilesSettings() {
  const { profiles, activeId, loading, save, remove, activate } = useProfiles()
  const [editing, setEditing] = useState<LoadedProfile | null>(null)
  const [confirmDelete, setConfirmDelete] = useState<LoadedProfile | null>(null)

  if (loading) {
    return <div className="py-4 text-xs text-muted-foreground">Loading your profiles…</div>
  }

  if (editing) {
    return (
      <ProfileEditor
        profile={editing}
        onCancel={() => setEditing(null)}
        onSave={async (p) => {
          if (await save(p)) {
            toast.success(`Saved "${p.label.trim()}".`)
            setEditing(null)
          }
        }}
      />
    )
  }

  return (
    <div>
      <p className="pt-3 text-2xs leading-relaxed text-muted-foreground">
        A profile is the name, email, and signing key that go on your commits. Switch profiles to
        change all three at once, so a work commit never goes out under your personal address.
      </p>

      {profiles.length === 0 ? (
        <div className="mt-4 rounded-lg border border-border bg-panel px-4 py-6 text-center">
          <UserRound size={20} className="mx-auto text-muted-foreground" />
          <div className="mt-2 text-xs font-semibold text-foreground">No profiles yet</div>
          <p className="mx-auto mt-1 max-w-sm text-2xs leading-relaxed text-muted-foreground">
            Make one for each identity you commit under, like Work and Personal. Your current git
            settings are used until you switch to a profile.
          </p>
          <div className="mt-3 flex items-center justify-center gap-2">
            <Button
              size="sm"
              className="h-8"
              onClick={async () => {
                // Start from what git already knows rather than making them
                // retype it. Nothing is written until they save. A failure to
                // read git is not worth blocking on -- open the blank form
                // rather than leaving the button looking dead (Rule #1).
                let seed = emptyProfile()
                try {
                  const res = await commands.profileFromCurrentConfig()
                  if (res.status === 'ok') seed = normalizeProfile({ ...res.data, id: newId() })
                } catch {
                  // Fall through to the blank form.
                }
                setEditing(seed)
              }}
            >
              <Plus size={13} />
              Make a profile
            </Button>
          </div>
          <p className="mt-2 text-[10px] text-muted-foreground">
            Starts from the name and email git already uses.
          </p>
        </div>
      ) : (
        <>
          <div className="mt-4 grid gap-2">
            {profiles.map((p) => (
              <ProfileCard
                key={p.id}
                profile={p}
                active={p.id === activeId}
                onUse={async () => {
                  if (await activate(p.id)) {
                    toast.success(`Now committing as ${p.name.trim()}.`)
                  }
                }}
                onEdit={() => setEditing(p)}
                onDelete={() => setConfirmDelete(p)}
              />
            ))}
          </div>
          <Button
            size="sm"
            variant="secondary"
            className="mt-3 h-8"
            onClick={() => setEditing(emptyProfile())}
          >
            <Plus size={13} />
            Add a profile
          </Button>
        </>
      )}

      <RepoOverride profiles={profiles} />

      <ConfirmDialog
        open={confirmDelete !== null}
        onOpenChange={(open) => !open && setConfirmDelete(null)}
        title={`Delete "${confirmDelete?.label ?? ''}"?`}
        description="The profile is removed and stops applying to its folders. Repositories you pinned to it keep the name and email they already have, and your git settings are not otherwise touched."
        confirmLabel="Delete profile"
        destructive
        onConfirm={async () => {
          const target = confirmDelete
          if (!target) return
          if (await remove(target.id)) toast.success(`Deleted "${target.label}".`)
          setConfirmDelete(null)
        }}
      />
    </div>
  )
}

function ProfileCard({
  profile,
  active,
  onUse,
  onEdit,
  onDelete,
}: {
  profile: LoadedProfile
  active: boolean
  onUse: () => void
  onEdit: () => void
  onDelete: () => void
}) {
  return (
    <div
      className={cn(
        'rounded-lg border px-3.5 py-3',
        active ? 'border-primary bg-primary/[.06]' : 'border-border bg-panel'
      )}
    >
      <div className="flex items-start gap-3">
        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-2">
            <span className="truncate text-xs font-semibold text-foreground">{profile.label}</span>
            {active && (
              <span className="flex flex-none items-center gap-1 rounded bg-primary/15 px-1.5 py-0.5 text-[10px] font-medium text-accent-text">
                <Check size={10} />
                In use
              </span>
            )}
          </div>
          <div className="mt-0.5 truncate text-2xs text-muted-foreground">
            {profile.name} &lt;{profile.email}&gt;
          </div>
          <div className="mt-1 flex flex-wrap items-center gap-x-3 gap-y-1 text-[10px] text-muted-foreground">
            <span className="flex items-center gap-1">
              <KeyRound size={10} />
              {describeSigning(profile)}
            </span>
            {profile.folders.length > 0 && (
              <span className="flex items-center gap-1">
                <Folder size={10} />
                {profile.folders.length === 1
                  ? profile.folders[0]
                  : `${profile.folders.length} folders`}
              </span>
            )}
          </div>
        </div>

        <div className="flex flex-none items-center gap-1">
          {!active && (
            <Button size="sm" variant="secondary" className="h-7" onClick={onUse}>
              Use
            </Button>
          )}
          <Button
            size="sm"
            variant="ghost"
            className="h-7 px-2 text-muted-foreground hover:text-foreground"
            tooltip="Edit profile"
            onClick={onEdit}
          >
            <Pencil size={12} />
          </Button>
          <Button
            size="sm"
            variant="ghost"
            className="h-7 px-2 text-muted-foreground hover:text-destructive"
            tooltip="Delete profile"
            onClick={onDelete}
          >
            <Trash2 size={12} />
          </Button>
        </div>
      </div>
    </div>
  )
}

function describeSigning(profile: LoadedProfile): string {
  if (profile.signing.kind === 'none') return 'Not signing'
  const how = profile.signing.kind === 'gpg' ? 'GPG key' : 'SSH key'
  return profile.signCommits ? `Signs with a ${how}` : `${how} set, signing off`
}

/* ------------------------------------------------------------------ editor */

function ProfileEditor({
  profile,
  onSave,
  onCancel,
}: {
  profile: LoadedProfile
  onSave: (p: LoadedProfile) => void
  onCancel: () => void
}) {
  const [draft, setDraft] = useState<LoadedProfile>(profile)
  const [gpgKeys, setGpgKeys] = useState<SigningKey[]>([])
  const [sshKeys, setSshKeys] = useState<SshKey[]>([])

  // Both key lists, so the picker can offer whatever the machine actually has.
  useEffect(() => {
    void (async () => {
      try {
        const [gpg, ssh] = await Promise.all([commands.listSigningKeys(), commands.listSshKeys()])
        if (gpg.status === 'ok') setGpgKeys(gpg.data)
        if (ssh.status === 'ok') setSshKeys(ssh.data)
      } catch {
        // No keys listed just means the picker offers "Do not sign"; the rest
        // of the form still works, so this is not worth an error toast.
      }
    })()
  }, [])

  const set = <K extends keyof LoadedProfile>(key: K, value: LoadedProfile[K]) =>
    setDraft((d) => ({ ...d, [key]: value }))

  const ready = draft.label.trim() && draft.name.trim() && draft.email.trim()

  // Signing a commit with a key whose address is not the commit's address gets
  // an unverified badge on the host, which is confusing to debug after the fact.
  const keyEmail = useMemo(() => signingKeyEmail(draft, gpgKeys), [draft, gpgKeys])
  const mismatch =
    keyEmail && draft.email.trim() && keyEmail !== draft.email.trim().toLowerCase()
      ? keyEmail
      : null

  return (
    <div className="pt-3">
      <SettingRow label="Profile name" hint="What you call this identity, like Work or Personal.">
        <Input
          value={draft.label}
          onChange={(e) => set('label', e.target.value)}
          placeholder="Personal"
          className="h-8 bg-background text-xs"
          aria-label="Profile name"
        />
      </SettingRow>

      <SettingRow label="Your name" hint="Goes on every commit made under this profile.">
        <Input
          value={draft.name}
          onChange={(e) => set('name', e.target.value)}
          placeholder="Your name"
          className="h-8 bg-background text-xs"
          aria-label="Name on commits"
        />
      </SettingRow>

      <SettingRow label="Your email" hint="Your host matches commits to your account by this.">
        <Input
          value={draft.email}
          onChange={(e) => set('email', e.target.value)}
          placeholder="you@example.com"
          className="h-8 bg-background text-xs"
          aria-label="Email on commits"
        />
      </SettingRow>

      <SettingRow
        label="Signing key"
        hint="Proves the commit came from you. Leave off if you are not signing yet."
      >
        <div className="grid gap-2">
          <select
            className={selectClass}
            value={signingValue(draft.signing)}
            aria-label="Signing key"
            onChange={(e) => set('signing', parseSigning(e.target.value))}
          >
            <option value="none">Do not sign</option>
            {gpgKeys.length > 0 && (
              <optgroup label="GPG keys">
                {gpgKeys.map((k) => (
                  <option key={k.id} value={`gpg:${k.id}`}>
                    {k.uid || k.id}
                  </option>
                ))}
              </optgroup>
            )}
            {sshKeys.length > 0 && (
              <optgroup label="SSH keys">
                {sshKeys.map((k) => (
                  <option key={k.path} value={`ssh:${k.path}`}>
                    {k.name}
                  </option>
                ))}
              </optgroup>
            )}
          </select>

          {draft.signing.kind !== 'none' && (
            <label className="flex cursor-pointer items-center gap-2 text-xs text-foreground">
              <input
                type="checkbox"
                checked={draft.signCommits}
                onChange={(e) => set('signCommits', e.target.checked)}
                className="size-3.5 accent-[var(--gw-accent)]"
              />
              Sign every commit made as this profile
            </label>
          )}

          {mismatch && (
            <div className="rounded-md border border-amber-500/40 bg-amber-500/10 px-2.5 py-2 text-2xs leading-relaxed text-muted-foreground">
              This key belongs to <span className="font-medium text-foreground">{mismatch}</span>,
              but this profile commits as{' '}
              <span className="font-medium text-foreground">{draft.email.trim()}</span>. Your host
              will not mark those commits verified.
            </div>
          )}
        </div>
      </SettingRow>

      <FolderRules
        folders={draft.folders}
        onChange={(folders) => set('folders', folders)}
      />

      <div className="mt-4 flex items-center gap-2">
        <Button size="sm" className="h-8" disabled={!ready} onClick={() => onSave(draft)}>
          Save profile
        </Button>
        <Button size="sm" variant="ghost" className="h-8" onClick={onCancel}>
          Cancel
        </Button>
      </div>
    </div>
  )
}

function signingValue(signing: SigningMethod): string {
  return signing.kind === 'none' ? 'none' : `${signing.kind}:${signing.value}`
}

function parseSigning(raw: string): SigningMethod {
  if (raw.startsWith('gpg:')) return { kind: 'gpg', value: raw.slice(4) }
  if (raw.startsWith('ssh:')) return { kind: 'ssh', value: raw.slice(4) }
  return { kind: 'none' }
}

/** Email on the chosen key, when we can tell. SSH keys carry no address. */
function signingKeyEmail(profile: LoadedProfile, gpgKeys: SigningKey[]): string | null {
  const signing = profile.signing
  if (signing.kind !== 'gpg') return null
  // Bound to a local first: narrowing a property does not survive into the
  // callback, where TypeScript has to assume it could have changed.
  const key = gpgKeys.find((k) => k.id === signing.value)
  return key?.uid.match(/<([^>]+)>/)?.[1]?.trim().toLowerCase() ?? null
}

/**
 * Folders whose repositories use this profile automatically.
 *
 * Written as git `includeIf` rules, so the terminal and other git tools resolve
 * the same identity GitWyrm shows.
 */
function FolderRules({
  folders,
  onChange,
}: {
  folders: string[]
  onChange: (folders: string[]) => void
}) {
  const add = async () => {
    const { open: openDialog } = await import('@tauri-apps/plugin-dialog')
    const dir = await openDialog({ directory: true, title: 'Pick a folder for this profile' })
    if (typeof dir === 'string' && !folders.includes(dir)) onChange([...folders, dir])
  }

  return (
    <SettingRow
      label="Use for these folders"
      searchId="profile-folders"
      hint="Repositories inside these folders use this profile automatically, in GitWyrm and in your terminal."
    >
      <div className="grid gap-2">
        {folders.length > 0 && (
          <div className="grid gap-1">
            {folders.map((folder) => (
              <div
                key={folder}
                className="flex items-center gap-2 rounded-md border border-border bg-background px-2.5 py-1.5"
              >
                <Folder size={12} className="flex-none text-muted-foreground" />
                <span className="min-w-0 flex-1 truncate font-mono text-2xs text-foreground">
                  {folder}
                </span>
                <button
                  onClick={() => onChange(folders.filter((f) => f !== folder))}
                  aria-label={`Stop using this profile for ${folder}`}
                  className="flex-none text-muted-foreground hover:text-destructive"
                >
                  <Trash2 size={12} />
                </button>
              </div>
            ))}
          </div>
        )}
        <Button size="sm" variant="secondary" className="h-8 justify-self-start" onClick={add}>
          <Plus size={12} />
          Add a folder
        </Button>
      </div>
    </SettingRow>
  )
}

/* --------------------------------------------------------- repo override */

/**
 * Pin the repository in the active tab to one profile.
 *
 * Writes that repository's own config, which outranks both the active profile
 * and any folder rule -- so this always wins, whatever else is set up.
 */
function RepoOverride({ profiles }: { profiles: LoadedProfile[] }) {
  const repo = useActiveRepo()
  const [effective, setEffective] = useState<{ email: string; overridden: boolean } | null>(null)

  const reload = async (path: string) => {
    const res = await commands.getEffectiveIdentity(path)
    if (res.status === 'ok') {
      setEffective({ email: res.data.email, overridden: res.data.overridden })
    }
  }

  useEffect(() => {
    if (repo?.path) void reload(repo.path)
    else setEffective(null)
  }, [repo?.path])

  if (!repo || profiles.length === 0) return null

  const apply = async (id: string) => {
    if (!id) {
      const res = await commands.clearRepoProfile(repo.path)
      if (res.status !== 'ok') return toast.error(res.error)
      toast.success('This repository follows your active profile again.')
    } else {
      const res = await commands.setRepoProfile(repo.path, id)
      if (res.status !== 'ok') return toast.error(res.error)
      const picked = profiles.find((p) => p.id === id)
      toast.success(`${repo.name} now commits as ${picked?.name ?? 'that profile'}.`)
    }
    await reload(repo.path)
  }

  return (
    <div className="mt-8">
      <h3 className="text-xs font-bold uppercase tracking-[.06em] text-sub">This repository</h3>
      <p className="mb-1 mt-1 max-w-prose text-2xs text-muted-foreground">
        Settings for {repo.name}, the repository open in the active tab.
      </p>

      <SettingRow
        label="Commit as"
        searchId="repo-profile"
        hint="Pin this one repository to a profile. It keeps that identity even when you switch profiles elsewhere."
      >
        <div className="grid gap-2">
          <select
            className={selectClass}
            value={''}
            aria-label="Profile for this repository"
            onChange={(e) => void apply(e.target.value)}
          >
            <option value="">Follow my active profile</option>
            {profiles.map((p) => (
              <option key={p.id} value={p.id}>
                Always use {p.label}
              </option>
            ))}
          </select>
          {effective && (
            <div className="text-2xs text-muted-foreground">
              Commits here are made as{' '}
              <span className="font-medium text-foreground">{effective.email || 'nobody yet'}</span>
              {effective.overridden ? ' (pinned to this repository).' : '.'}
            </div>
          )}
        </div>
      </SettingRow>
    </div>
  )
}
