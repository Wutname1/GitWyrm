import type { ReactNode } from 'react'
import { Copy, ExternalLink, Mail } from 'lucide-react'
import { copyToClipboard } from '@/lib/clipboard'
import { authorColor } from '@/lib/gitDisplay'
import { authorProfileLink } from '@/lib/authorProfile'
import { openWebUrl, remoteWebTarget } from '@/lib/remoteWeb'
import { useRemotes } from '@/hooks/useGitQueries'
import { useUiStore } from '@/stores/uiStore'
import { useActiveRepo } from '@/stores/workspaceStore'
import { HoverCard, HoverCardContent, HoverCardTrigger } from '@/components/ui/hover-card'
import { Avatar } from './Avatar'

interface AuthorHoverCardProps {
  name: string
  email: string
  initials: string
  /** Commit this author cell belongs to; clicking the card selects it. */
  sha?: string
  children: ReactNode
}

/** Author details on hover: click-to-copy email plus a link out to the host. */
export function AuthorHoverCard({ name, email, initials, sha, children }: AuthorHoverCardProps) {
  const selectCommit = useUiStore((s) => s.selectCommit)
  const repo = useActiveRepo()
  const remotes = useRemotes(repo?.id ?? null)
  const primary = remotes.data?.find((r) => r.name === 'origin') ?? remotes.data?.[0]
  const target = primary ? remoteWebTarget(primary.url) : null
  const profile = target ? authorProfileLink(target.provider, email, target.repositoryUrl) : null
  const color = authorColor(email || name)

  return (
    <HoverCard>
      <HoverCardTrigger asChild>{children}</HoverCardTrigger>
      <HoverCardContent>
        <button
          type="button"
          disabled={!sha}
          onClick={() => sha && selectCommit(sha)}
          title={sha ? 'Show this commit in the details pane' : undefined}
          className="-m-1 flex w-[calc(100%+0.5rem)] items-center gap-2.5 rounded-[5px] p-1 text-left enabled:hover:bg-soft"
        >
          <Avatar initials={initials} color={color} email={email} size="md" />
          <div className="min-w-0 flex-1">
            <div className="overflow-hidden text-ellipsis whitespace-nowrap text-xs font-semibold text-foreground">
              {name}
            </div>
            {email && (
              <div className="overflow-hidden text-ellipsis whitespace-nowrap text-2xs text-muted-foreground">
                {email}
              </div>
            )}
          </div>
        </button>

        <div className="mt-2.5 flex flex-col gap-1">
          {email && (
            <>
              <button
                type="button"
                onClick={() => void copyToClipboard(email, 'Email address copied')}
                className="flex items-center gap-2 rounded-[5px] px-2 py-1.5 text-2xs text-sub hover:bg-soft hover:text-foreground"
              >
                <Copy className="size-3.5 flex-none" />
                Copy email address
              </button>
              <button
                type="button"
                onClick={() => openWebUrl(`mailto:${email}`, 'your email app')}
                className="flex items-center gap-2 rounded-[5px] px-2 py-1.5 text-2xs text-sub hover:bg-soft hover:text-foreground"
              >
                <Mail className="size-3.5 flex-none" />
                Send an email
              </button>
            </>
          )}
          {profile && (
            <button
              type="button"
              onClick={() => openWebUrl(profile.url, profile.label)}
              className="flex items-center gap-2 rounded-[5px] px-2 py-1.5 text-2xs text-sub hover:bg-soft hover:text-foreground"
            >
              <ExternalLink className="size-3.5 flex-none" />
              {profile.label}
            </button>
          )}
        </div>
      </HoverCardContent>
    </HoverCard>
  )
}
