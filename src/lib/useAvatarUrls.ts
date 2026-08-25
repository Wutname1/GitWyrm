import { useEffect, useState } from 'react'
import { avatarUrl } from './avatarSource'
import { botIdentity } from './brandLogos'

/** A resolved author picture, and what kind of thing it is. */
export interface ResolvedAvatar {
  url: string
  /** A flat tool mark rather than a photo, so it needs a disc behind it. */
  bot: boolean
  /** A silhouette with no color of its own; must be repainted to be visible. */
  mono: boolean
}

/**
 * Resolved avatar image URLs for a set of commit author emails.
 *
 * The graph draws its nodes in one SVG rather than as a component per row, so
 * it cannot lean on `<Avatar>`'s own effect: it needs every visible author
 * resolved together. Resolution is per-email and cached by `avatarSource`, so
 * scrolling back over rows already seen costs nothing.
 *
 * Bots resolve to their tool's mark, which is a local asset and needs no probe.
 * The caller is told it is a bot, because a bot mark is a flat single-color
 * glyph rather than a photo: it needs its own disc behind it and, when it is a
 * silhouette, recoloring to read on a dark surface.
 *
 * @param emails Author emails currently on screen. Order does not matter.
 * @param px Pixel size to request, before device scaling.
 * @returns Map from lowercased email to its picture. A missing entry means the
 *   lookup has not finished or the author has no picture anywhere.
 */
export function useAvatarUrls(emails: string[], px: number): Map<string, ResolvedAvatar> {
  const [urls, setUrls] = useState<Map<string, ResolvedAvatar>>(new Map())
  // Depending on the array itself would re-run on every render, since the
  // caller rebuilds it each time. The joined key changes only when the set of
  // authors on screen actually changes.
  const key = [...new Set(emails.map((e) => e.trim().toLowerCase()))].sort().join('\n')

  useEffect(() => {
    if (!key) return
    let cancelled = false
    const wanted = key.split('\n')

    void Promise.all(
      wanted.map(async (email): Promise<readonly [string, ResolvedAvatar | null]> => {
        const bot = botIdentity(email)
        if (bot) return [email, { url: bot.logo, bot: true, mono: bot.mono }]
        const url = await avatarUrl(email, px)
        return [email, url ? { url, bot: false, mono: false } : null]
      }),
    ).then((pairs) => {
      if (cancelled) return
      setUrls((prev) => {
        // Keep entries for authors that scrolled off: they cost nothing to
        // hold and scrolling back would otherwise flash the fallback disc.
        const next = new Map(prev)
        let changed = false
        for (const [email, resolved] of pairs) {
          if (resolved && next.get(email)?.url !== resolved.url) {
            next.set(email, resolved)
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
