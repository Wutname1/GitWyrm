import { useCallback, useRef, useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { commands } from '@/lib/bindings'
import { isTauri } from '@/lib/env'
import { unwrap } from '@/lib/queryKeys'

const catalogKey = ['ai-catalog'] as const
const configuredKey = ['ai-configured'] as const

export function useAiCatalog(enabled = true) {
  return useQuery({
    queryKey: catalogKey,
    enabled: enabled && isTauri,
    staleTime: 60 * 60 * 1000,
    queryFn: async () => unwrap(await commands.aiGetCatalog()),
  })
}

export function useAiDefaultInstruction() {
  return useQuery({
    queryKey: ['ai-default-instruction'],
    enabled: isTauri,
    staleTime: Infinity,
    queryFn: async () => await commands.aiDefaultInstruction(),
  })
}

export function useAiConfigured() {
  return useQuery({
    queryKey: configuredKey,
    enabled: isTauri,
    queryFn: async () => unwrap(await commands.aiListConfigured()),
  })
}

/**
 * Models the user can actually use for a provider. Refetches when the
 * provider's configured state changes so newly-connected keys reveal their
 * real (e.g. Copilot plan-gated) model list. `configured` is only part of the
 * key so a fresh sign-in busts the cache.
 */
export function useAiModels(providerId: string | null, configured: boolean) {
  return useQuery({
    queryKey: ['ai-models', providerId, configured],
    enabled: isTauri && providerId != null,
    staleTime: 5 * 60 * 1000,
    queryFn: async () => unwrap(await commands.aiListModels(providerId!)),
  })
}

export function useAiMutations() {
  const qc = useQueryClient()
  const invalidate = () => qc.invalidateQueries({ queryKey: configuredKey })

  return {
    setApiKey: useMutation({
      mutationFn: async (v: { provider: string; key: string }) =>
        unwrap(await commands.aiSetApiKey(v.provider, v.key)),
      onSuccess: invalidate,
    }),
    removeProvider: useMutation({
      mutationFn: async (provider: string) => unwrap(await commands.aiRemoveProvider(provider)),
      onSuccess: invalidate,
    }),
    generate: useMutation({
      mutationFn: async (v: { repoId: string; provider: string; model: string }) =>
        unwrap(await commands.generateCommitMessage(v.repoId, v.provider, v.model)),
    }),
  }
}

export type CopilotSignIn =
  | { state: 'idle' }
  | { state: 'starting' }
  | { state: 'waiting'; userCode: string; verificationUri: string }
  | { state: 'error'; message: string }

/** Drives the GitHub Copilot device-code sign-in: start, open browser, poll. */
export function useCopilotSignIn() {
  const qc = useQueryClient()
  const [status, setStatus] = useState<CopilotSignIn>({ state: 'idle' })
  const cancelled = useRef(false)

  const cancel = useCallback(() => {
    cancelled.current = true
    setStatus({ state: 'idle' })
  }, [])

  const start = useCallback(async () => {
    cancelled.current = false
    setStatus({ state: 'starting' })
    try {
      const info = unwrap(await commands.aiCopilotDeviceStart())
      setStatus({
        state: 'waiting',
        userCode: info.user_code,
        verificationUri: info.verification_uri,
      })
      const { openUrl } = await import('@tauri-apps/plugin-opener')
      await openUrl(info.verification_uri)

      let interval = Math.max(info.interval, 5)
      for (;;) {
        await new Promise((r) => setTimeout(r, interval * 1000))
        if (cancelled.current) return
        const poll = unwrap(await commands.aiCopilotDevicePoll(info.device_code, interval))
        if (poll.status === 'complete') {
          setStatus({ state: 'idle' })
          qc.invalidateQueries({ queryKey: configuredKey })
          return
        }
        interval = Math.max(poll.interval, 5)
      }
    } catch (e) {
      if (!cancelled.current) setStatus({ state: 'error', message: String(e) })
    }
  }, [qc])

  return { status, start, cancel }
}
