import { useEffect, useState } from 'react'
import { cn } from '@/lib/utils'
import { avatarUrl } from '@/lib/avatarSource'
import { GithubIcon } from '@/components/domain/github/GithubIcon'

interface AvatarProps {
  initials: string
  color: string
  email?: string
  size?: 'sm' | 'md'
}

/**
 * Dependabot commits are authored by GitHub itself, not a person, so the
 * two-letter initials ("DE") read as a stranger on the team. Show the GitHub
 * mark instead. Its address is always the bot's no-reply one, in either the
 * numeric-id form or the older bare-login form.
 */
export function isDependabot(email: string): boolean {
  return /^(?:\d+\+)?dependabot(?:\[bot\])?@users\.noreply\.github\.com$/i.test(email.trim())
}

export function Avatar({ initials, color, email, size = 'sm' }: AvatarProps) {
  const px = size === 'sm' ? 19 : 26
  const bot = !!email && isDependabot(email)
  const [src, setSrc] = useState<string | null>(null)

  useEffect(() => {
    if (!email || bot) return
    let cancelled = false
    void avatarUrl(email, px * 2).then((url) => {
      if (!cancelled) setSrc(url)
    })
    return () => {
      cancelled = true
    }
  }, [email, px, bot])

  if (bot) {
    return (
      <span
        className={cn(
          'flex flex-none items-center justify-center rounded-full text-foreground',
          size === 'sm' ? 'size-[19px]' : 'size-[26px]'
        )}
        style={{ background: color + '2b', border: `1px solid ${color}55` }}
      >
        <GithubIcon size={size === 'sm' ? 12 : 16} />
      </span>
    )
  }

  if (src) {
    return (
      <img
        src={src}
        alt=""
        className={cn('flex-none rounded-full', size === 'sm' ? 'size-[19px]' : 'size-[26px]')}
        style={{ border: `1px solid ${color}55` }}
      />
    )
  }

  return (
    <span
      className={cn(
        'flex flex-none items-center justify-center rounded-full font-bold',
        size === 'sm' ? 'size-[19px] text-2xs' : 'size-[26px] text-2xs'
      )}
      style={{ background: color + '2b', color, border: `1px solid ${color}55` }}
    >
      {initials}
    </span>
  )
}
