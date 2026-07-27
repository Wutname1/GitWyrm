import { useCallback, useEffect, useState } from 'react'
import { toast } from 'sonner'
import { commands, type Profile } from '@/lib/bindings'

/**
 * The user's named identities and which one is active.
 *
 * Profiles live in GitWyrm's settings, but applying one writes real git config.
 * Every mutation here re-reads rather than patching local state, so the screen
 * always shows what git will actually do rather than what we hoped it would.
 */
export function useProfiles() {
  const [profiles, setProfiles] = useState<Profile[]>([])
  const [activeId, setActiveId] = useState<string | null>(null)
  const [loading, setLoading] = useState(true)

  const reload = useCallback(async () => {
    try {
      const [list, active] = await Promise.all([
        commands.listProfiles(),
        commands.getActiveProfileId(),
      ])
      if (list.status === 'ok') setProfiles(list.data)
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
      setProfiles(res.data)
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
    setProfiles(res.data)
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
