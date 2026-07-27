import { useCallback, useEffect, useState } from 'react'
import { commands, type GitIdentity } from '@/lib/bindings'

/**
 * The name and email git puts on commits, read from the user's git config.
 *
 * Deliberately not mirrored into GitWyrm's own settings: this belongs to git,
 * is shared with every other tool on the machine, and a second copy would drift
 * the moment someone edits .gitconfig by hand.
 */
export function useGitIdentity() {
  const [identity, setIdentity] = useState<GitIdentity | null>(null)
  const [loading, setLoading] = useState(true)

  const reload = useCallback(async () => {
    setLoading(true)
    const res = await commands.getGitIdentity()
    setLoading(false)
    if (res.status === 'ok') {
      setIdentity(res.data)
      return res.data
    }
    setIdentity(null)
    return null
  }, [])

  useEffect(() => {
    void reload()
  }, [reload])

  const save = useCallback(
    async (name: string, email: string) => {
      const res = await commands.setGitIdentity(name, email)
      if (res.status !== 'ok') return { ok: false as const, error: res.error }
      // Re-read rather than trusting the values we sent: git trims and may
      // normalize, and the UI should show what git actually stored.
      await reload()
      return { ok: true as const }
    },
    [reload]
  )

  /** Both halves present, which is what git needs before it allows a commit. */
  const isComplete = Boolean(identity?.name.trim() && identity?.email.trim())

  return { identity, loading, isComplete, reload, save }
}
