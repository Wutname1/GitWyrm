import { useEffect, useState } from 'react'
import { commands } from '@/lib/bindings'
import { parseSemver } from '@/lib/semver'

const inTauri = '__TAURI_INTERNALS__' in window

/**
 * Whether this build is a pre-release. The beta workflow compiles a version of
 * "X.Y.Z-beta.N" into the binary, so a non-empty pre-release list is the signal.
 * The update *channel* setting is only a preference about future downloads and
 * says nothing about the build already running.
 *
 * The answer cannot change while the app is open, so it is resolved once and
 * shared by every caller instead of costing an IPC round-trip per component.
 */
let cached: boolean | undefined
let inFlight: Promise<boolean> | undefined

function resolveIsBeta(): Promise<boolean> {
  if (cached !== undefined) return Promise.resolve(cached)
  inFlight ??= commands
    .buildInfo()
    .then((info) => (parseSemver(info.version)?.pre.length ?? 0) > 0)
    .catch(() => false)
    .then((value) => {
      cached = value
      return value
    })
  return inFlight
}

export function useIsBetaBuild(): boolean {
  const [isBeta, setIsBeta] = useState(cached ?? false)

  useEffect(() => {
    if (!inTauri || cached !== undefined) return
    let active = true
    void resolveIsBeta().then((value) => {
      if (active) setIsBeta(value)
    })
    return () => {
      active = false
    }
  }, [])

  return isBeta
}
