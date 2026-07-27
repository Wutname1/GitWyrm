import { useCallback, useEffect, useState } from 'react'
import { toast } from 'sonner'
import { commands, type Profile } from '@/lib/bindings'

/**
 * A profile with the always-present fields actually marked as present.
 *
 * `signing` and `folders` are `#[serde(default)]` on the Rust side, which is
 * about tolerating older settings files on the way in -- the backend always
 * sends both. specta renders any `serde(default)` field as optional, so the
 * generated `Profile` overstates it and every read site would otherwise have
 * to defend against an `undefined` that never arrives.
 */
export type LoadedProfile = Profile & {
  signing: NonNullable<Profile['signing']>
  folders: NonNullable<Profile['folders']>
}

/** Fill in what the backend guarantees but the generated type does not. */
export function normalizeProfile(profile: Profile): LoadedProfile {
  return {
    ...profile,
    signing: profile.signing ?? { kind: 'none' },
    folders: profile.folders ?? [],
  }
}

/**
 * The user's named identities and which one is active.
 *
 * Profiles live in GitWyrm's settings, but applying one writes real git config.
 * Every mutation here re-reads rather than patching local state, so the screen
 * always shows what git will actually do rather than what we hoped it would.
 */
export function useProfiles() {
  const [profiles, setProfiles] = useState<LoadedProfile[]>([])
  const [activeId, setActiveId] = useState<string | null>(null)
  const [loading, setLoading] = useState(true)

  const reload = useCallback(async () => {
    try {
      const [list, active] = await Promise.all([
        commands.listProfiles(),
        commands.getActiveProfileId(),
      ])
      if (list.status === 'ok') setProfiles(list.data.map(normalizeProfile))
      else toast.error(list.error)
      if (active.status === 'ok') setActiveId(active.data)
    } catch (e) {
      // Without this the screen sits on "Loading your profiles..." forever,
      // which reads as a hang rather than a failure worth reporting.
      toast.error(`Could not read your profiles: ${String(e)}`)
    } finally {
      setLoading(false)
    }
  }, [])

  useEffect(() => {
    void reload()
  }, [reload])

  const save = useCallback(
    async (profile: Profile) => {
      const res = await commands.saveProfile(profile)
      if (res.status !== 'ok') {
        toast.error(res.error)
        return false
      }
      setProfiles(res.data.map(normalizeProfile))
      return true
    },
    []
  )

  const remove = useCallback(async (id: string) => {
    const res = await commands.deleteProfile(id)
    if (res.status !== 'ok') {
      toast.error(res.error)
      return false
    }
    setProfiles(res.data.map(normalizeProfile))
    await reload()
    return true
  }, [reload])

  const activate = useCallback(
    async (id: string) => {
      const res = await commands.setActiveProfile(id)
      if (res.status !== 'ok') {
        toast.error(res.error)
        return false
      }
      setActiveId(id)
      return true
    },
    []
  )

  return { profiles, activeId, loading, reload, save, remove, activate }
}
