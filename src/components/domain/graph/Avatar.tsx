import { useEffect, useState } from 'react'
import { cn } from '@/lib/utils'
import { avatarUrl } from '@/lib/avatarSource'

interface AvatarProps {
  initials: string
  color: string
  email?: string
  size?: 'sm' | 'md'
}

export function Avatar({ initials, color, email, size = 'sm' }: AvatarProps) {
  const px = size === 'sm' ? 19 : 26
  const [src, setSrc] = useState<string | null>(null)

  useEffect(() => {
    if (!email) return
    let cancelled = false
    void avatarUrl(email, px * 2).then((url) => {
      if (!cancelled) setSrc(url)
    })
    return () => {
      cancelled = true
    }
  }, [email, px])

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
