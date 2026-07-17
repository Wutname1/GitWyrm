import { useEffect, useState } from 'react'
import { cn } from '@/lib/utils'

// Gravatar accepts SHA-256 email hashes (d=404 -> 404 when no avatar exists).
// Cache hash + existence per email for the session so scrolling stays cheap.
const urlCache = new Map<string, Promise<string | null>>()

async function gravatarUrl(email: string, px: number): Promise<string | null> {
  const normalized = email.trim().toLowerCase()
  if (!normalized) return null
  const bytes = new TextEncoder().encode(normalized)
  const digest = await crypto.subtle.digest('SHA-256', bytes)
  const hash = [...new Uint8Array(digest)].map((b) => b.toString(16).padStart(2, '0')).join('')
  const url = `https://gravatar.com/avatar/${hash}?s=${px}&d=404`
  return new Promise((resolve) => {
    const img = new Image()
    img.onload = () => resolve(url)
    img.onerror = () => resolve(null)
    img.src = url
  })
}

function lookup(email: string, px: number): Promise<string | null> {
  const key = email.trim().toLowerCase()
  let hit = urlCache.get(key)
  if (!hit) {
    hit = gravatarUrl(key, px)
    urlCache.set(key, hit)
  }
  return hit
}

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
    lookup(email, px * 2).then((url) => {
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
        size === 'sm' ? 'size-[19px] text-[8px]' : 'size-[26px] text-[9.5px]'
      )}
      style={{ background: color + '2b', color, border: `1px solid ${color}55` }}
    >
      {initials}
    </span>
  )
}
