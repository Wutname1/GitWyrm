import { useEffect, useState } from 'react'
import { Check, Pin, UserRound } from 'lucide-react'
import { toast } from 'sonner'
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu'
import { useProfiles } from '@/hooks/useProfiles'
import { commands, type GitIdentitySnapshot } from '@/lib/bindings'
import { cn } from '@/lib/utils'
import { useUiStore } from '@/stores/uiStore'
import { useActiveRepo } from '@/stores/workspaceStore'

/**
 * Who the next commit will be authored as, right above the commit button.
 *
 * Placed at the point of decision rather than in a title bar: the identity on a
 * commit is only wrong in a way that matters once it is committed, and by then
 * fixing it means rewriting history. Shows the identity git actually resolves,
 * read from git rather than from our settings, so a hand-edited config or a
 * folder rule we did not write still tells the truth.
 */
export function ProfileSwitcher() {
  const repo = useActiveRepo()
  const { profiles, activeId, activate, reload } = useProfiles()
  const [effective, setEffective] = useState<GitIdentitySnapshot | null>(null)
  const showSettings = useUiStore((s) => s.showSettings)

  const repoPath = repo?.path ?? null

  const readEffective = async (path: string) => {
    const res = await commands.getEffectiveIdentity(path)
    setEffective(res.status === 'ok' ? res.data : null)
  }

  useEffect(() => {
    if (repoPath) void readEffective(repoPath)
    else setEffective(null)
  }, [repoPath])

  if (!repoPath) return null

  // Nothing configured at all: say so plainly, because git will refuse the
  // commit and the error it gives names config keys most people never set.
  const missing = !effective?.email

  // With one identity and nothing unusual about it there is no decision to
  // surface, and a line restating the only possible answer above every commit
  // is noise. It comes back the moment the answer could surprise: more than one
  // profile to choose between, a repository pinned to something other than the
  // active profile, or nothing set up at all.
  const worthShowing = missing || profiles.length > 1 || Boolean(effective?.overridden)
  if (!worthShowing) return null

  // The email, not the profile label: it is what a host matches commits by, and
  // it stays meaningful even when the label is something the user never chose.
  const label = missing ? 'Set who you commit as' : effective.email

  const pick = async (id: string) => {
    if (!(await activate(id))) return
    const chosen = profiles.find((p) => p.id === id)
    toast.success(`Now committing as ${chosen?.name ?? 'that profile'}.`)
    await readEffective(repoPath)
  }

  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <button
          className={cn(
            'mb-[7px] flex w-full items-center gap-1.5 rounded-md border px-2 py-1 text-left transition-colors',
            missing
              ? 'border-amber-500/50 bg-amber-500/10 hover:border-amber-500'
              : 'border-transparent bg-panel3/60 hover:border-border'
          )}
          title={
            effective?.email
              ? `Commits here are made as ${effective.name} <${effective.email}>`
              : 'No name and email set yet'
          }
        >
          <UserRound
            size={11}
            className={missing ? 'flex-none text-amber-500' : 'flex-none text-muted-foreground'}
          />
          <span
            className={cn(
              'min-w-0 flex-1 truncate text-[10px]',
              missing ? 'font-medium text-amber-500' : 'text-muted-foreground'
            )}
          >
            {missing ? label : <span className="text-foreground">{label}</span>}
          </span>
          {effective?.overridden && (
            <Pin size={9} className="flex-none text-muted-foreground" aria-label="Pinned to this repository" />
          )}
        </button>
      </DropdownMenuTrigger>

      <DropdownMenuContent align="start" className="w-64">
        {effective?.email && (
          <div className="px-2 py-1.5 text-2xs text-muted-foreground">
            <div className="truncate text-foreground">{effective.name}</div>
            <div className="truncate">{effective.email}</div>
            {effective.overridden && (
              <div className="mt-1 flex items-center gap-1 text-[10px]">
                <Pin size={9} />
                Pinned to this repository
              </div>
            )}
          </div>
        )}
        {effective?.email && profiles.length > 0 && <DropdownMenuSeparator />}

        {profiles.map((p) => (
          <DropdownMenuItem key={p.id} onSelect={() => void pick(p.id)}>
            <Check
              size={12}
              className={cn('flex-none', p.id === activeId ? 'opacity-100' : 'opacity-0')}
            />
            <span className="min-w-0 flex-1 truncate">{p.label}</span>
            <span className="flex-none text-[10px] text-muted-foreground">{p.email}</span>
          </DropdownMenuItem>
        ))}

        {profiles.length > 0 && <DropdownMenuSeparator />}
        <DropdownMenuItem
          onSelect={() => {
            showSettings('profiles')
            void reload()
          }}
        >
          {profiles.length === 0 ? 'Set up profiles…' : 'Manage profiles…'}
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  )
}
