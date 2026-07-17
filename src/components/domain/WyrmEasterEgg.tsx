import { useCallback, useEffect, useRef, useState } from 'react'
import { createPortal } from 'react-dom'
import logoUrl from '@/assets/logo.png'

/** Number of rapid clicks that triggers the full-screen logo explosion. */
const EXPLOSION_THRESHOLD = 10
/** Clicks must land within this window (ms) of each other to keep counting. */
const CLICK_WINDOW = 600

const LETTERS = [
  { ch: 'G', color: '#D7DEE7' },
  { ch: 'i', color: '#D7DEE7' },
  { ch: 't', color: '#D7DEE7' },
  { ch: 'W', color: '#2DD4A7' },
  { ch: 'y', color: '#2DD4A7' },
  { ch: 'r', color: '#2DD4A7' },
  { ch: 'm', color: '#2DD4A7' },
]

interface Shard {
  id: number
  ch: string
  color: string
  tx: number
  ty: number
  rot: number
  delay: number
  size: number
}

/**
 * Handles the playful GitWyrm logo interactions:
 * - a single click springs the logo (bounce), driven by the `bounce` return value
 * - ten rapid clicks blow a giant logo out of the app center and scatter the letters
 *
 * Render <WyrmExplosion /> once near the app root and drive it via useWyrmEasterEgg.
 */
export function useWyrmEasterEgg() {
  const [bounceNonce, setBounceNonce] = useState(0)
  const [blast, setBlast] = useState(0)
  const clicks = useRef(0)
  const lastClick = useRef(0)
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null)

  const onLogoClick = useCallback(() => {
    // performance.now avoids Date.now() and is monotonic for click cadence.
    const now = performance.now()
    clicks.current = now - lastClick.current < CLICK_WINDOW ? clicks.current + 1 : 1
    lastClick.current = now

    setBounceNonce((n) => n + 1)

    if (clicks.current >= EXPLOSION_THRESHOLD) {
      clicks.current = 0
      setBlast((b) => b + 1)
    }

    if (timer.current) clearTimeout(timer.current)
    timer.current = setTimeout(() => {
      clicks.current = 0
    }, CLICK_WINDOW)
  }, [])

  useEffect(() => () => void (timer.current && clearTimeout(timer.current)), [])

  return { onLogoClick, bounceNonce, blast }
}

/** Full-screen explosion overlay. `blast` is a nonce; each increment fires once. */
export function WyrmExplosion({ blast }: { blast: number }) {
  const [shards, setShards] = useState<Shard[]>([])
  const [showLogo, setShowLogo] = useState(false)

  useEffect(() => {
    if (blast === 0) return

    const w = window.innerWidth
    const h = window.innerHeight
    // Deterministic-ish spread seeded off the blast count so each run differs
    // without Math.random (unavailable) — jitter comes from the letter index.
    const next: Shard[] = []
    for (let pass = 0; pass < 5; pass++) {
      LETTERS.forEach((l, i) => {
        const seed = (blast * 97 + pass * 31 + i * 53) % 360
        const angle = (seed / 360) * Math.PI * 2
        const dist = 240 + ((seed * 7) % 420)
        next.push({
          id: pass * 100 + i,
          ch: l.ch,
          color: l.color,
          tx: Math.cos(angle) * dist + (w / 2 - w / 2),
          ty: Math.sin(angle) * dist,
          rot: (seed % 2 ? 1 : -1) * (180 + (seed % 540)),
          delay: pass * 40 + i * 20,
          size: 34 + ((seed * 3) % 60),
        })
      })
    }

    setShards(next)
    setShowLogo(true)
    const t1 = setTimeout(() => setShowLogo(false), 10500)
    const t2 = setTimeout(() => setShards([]), 2600)
    return () => {
      clearTimeout(t1)
      clearTimeout(t2)
    }
  }, [blast])

  if (shards.length === 0 && !showLogo) return null

  return createPortal(
    <div className="pointer-events-none fixed inset-0 z-[9999] overflow-hidden">
      {showLogo && (
        <img
          src={logoUrl}
          alt=""
          draggable={false}
          className="absolute left-1/2 top-1/2 size-[26rem] -translate-x-1/2 -translate-y-1/2 wyrm-blast-logo"
        />
      )}
      {shards.map((s) => (
        <span
          key={`${blast}-${s.id}`}
          className="wyrm-shard absolute left-1/2 top-1/2 font-black"
          style={
            {
              color: s.color,
              fontFamily: 'var(--font-wordmark)',
              fontSize: `${s.size}px`,
              '--tx': `${s.tx}px`,
              '--ty': `${s.ty}px`,
              '--rot': `${s.rot}deg`,
              animationDelay: `${s.delay}ms`,
            } as React.CSSProperties
          }
        >
          {s.ch}
        </span>
      ))}
    </div>,
    document.body
  )
}
