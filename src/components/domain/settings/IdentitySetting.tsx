import { useEffect, useState } from 'react'
import { Check, Loader2 } from 'lucide-react'
import { toast } from 'sonner'
import { Input } from '@/components/ui/input'
import { useGitIdentity } from '@/lib/useGitIdentity'

/**
 * The name and email stamped on every commit.
 *
 * Saved to the user's global git config, so it applies everywhere and matches
 * what they would get from `git config --global`. Both fields save together:
 * git needs both before it will make a commit, so saving one at a time would
 * leave people in a state that still refuses to commit.
 */
export function IdentitySetting() {
  const { identity, loading, save } = useGitIdentity()
  const [name, setName] = useState('')
  const [email, setEmail] = useState('')
  const [saving, setSaving] = useState(false)
  const [saved, setSaved] = useState(false)

  // Fill the boxes once the real values arrive.
  useEffect(() => {
    if (identity) {
      setName(identity.name)
      setEmail(identity.email)
    }
  }, [identity])

  const dirty = identity ? name !== identity.name || email !== identity.email : false

  const commit = async () => {
    if (!dirty || saving) return
    // An incomplete pair is the state git rejects, so don't write it.
    if (!name.trim() || !email.trim()) return

    setSaving(true)
    const res = await save(name, email)
    setSaving(false)

    if (!res.ok) {
      toast.error(res.error)
      return
    }
    // Rule #1: the row itself acknowledges the save, not just a toast.
    setSaved(true)
    setTimeout(() => setSaved(false), 2000)
  }

  if (loading) {
    return (
      <div className="flex h-8 items-center gap-1.5 text-2xs text-muted-foreground">
        <Loader2 size={12} className="animate-spin" />
        Checking...
      </div>
    )
  }

  return (
    <div className="grid gap-1.5">
      <div className="grid gap-2 sm:grid-cols-2">
        <Input
          value={name}
          onChange={(e) => setName(e.target.value)}
          onBlur={commit}
          onKeyDown={(e) => e.key === 'Enter' && commit()}
          placeholder="Your name"
          className="h-8 bg-background text-xs"
          aria-label="Name on your commits"
        />
        <Input
          value={email}
          onChange={(e) => setEmail(e.target.value)}
          onBlur={commit}
          onKeyDown={(e) => e.key === 'Enter' && commit()}
          placeholder="you@example.com"
          className="h-8 bg-background text-xs"
          aria-label="Email on your commits"
        />
      </div>
      <StatusLine saving={saving} saved={saved} name={name} email={email} />
    </div>
  )
}

function StatusLine({
  saving,
  saved,
  name,
  email,
}: {
  saving: boolean
  saved: boolean
  name: string
  email: string
}) {
  if (saving) {
    return (
      <div className="flex items-center gap-1.5 text-2xs text-muted-foreground">
        <Loader2 size={12} className="animate-spin" />
        Saving...
      </div>
    )
  }
  if (saved) {
    return (
      <div className="flex items-center gap-1.5 text-2xs text-emerald-500">
        <Check size={12} />
        Saved
      </div>
    )
  }
  if (!name.trim() || !email.trim()) {
    return (
      <div className="text-2xs text-amber-500">
        Git needs both before it can make a commit.
      </div>
    )
  }
  return null
}
