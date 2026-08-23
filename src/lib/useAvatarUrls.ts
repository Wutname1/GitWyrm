import { useEffect, useState } from 'react'
import { avatarUrl } from './avatarSource'
import { botIdentity } from './brandLogos'

/**
 * Resolved avatar image URLs for a set of commit author emails.
 *
 * The graph draws its nodes in one SVG rather than as a component per row, so
 * it cannot lean on `<Avatar>`'s own effect: it needs every visible author
 * resolved together. Resolution is per-email and cached by `avatarSource`, so
 * scrolling back over rows already seen costs nothing.
 *
 * Bots resolve to their tool's mark, which is a local asset and needs no probe.
 * Silhouette marks are skipped: they carry no color of their own and would
 * render as a black disc on a dark graph, so those authors keep their initials.
 *
 * @param emails Author emails currently on screen. Order does not matter.
 * @param px Pixel size to request, before device scaling.
 * @returns Map from lowercased email to image URL. A missing entry means the
 *   lookup has not finished or the author has no picture anywhere.
 */
export function useAvatarUrls(emails: string[], px: number): Map<string, string> {
  const [urls, setUrls] = useState<Map<string, string>>(new Map())
  // Depending on the array itself would re-run on every render, since the
  // caller rebuilds it each time. The joined key changes only when the set of
  // authors on screen actually changes.
  const key = [...new Set(emails.map((e) => e.trim().toLowerCase()))].sort().join('\n')

  useEffect(() => {
    if (!key) return
    let cancelled = false
    const wanted = key.split('\n')

    void Promise.all(
      wanted.map(async (email) => {
        const bot = botIdentity(email)
        if (bot) return [email, bot.mono ? null : bot.logo] as const
        const url = await avatarUrl(email, px)
        return [email, url] as const
      }),
    ).then((pairs) => {
      if (cancelled) return
      setUrls((prev) => {
        // Keep entries for authors that scrolled off: they cost nothing to
        // hold and scrolling back would otherwise flash the fallback disc.
        const next = new Map(prev)
        let changed = false
        for (const [email, url] of pairs) {
          if (url && next.get(email) !== url) {
            next.set(email, url)
            changed = true
          }
        }
        return changed ? next : prev
      })
    })

    return () => {
      cancelled = true
    }
  }, [key, px])

  return urls
}
