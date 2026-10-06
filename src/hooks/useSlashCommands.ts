import { useQuery } from '@tanstack/react-query'
import { commands } from '@/lib/bindings'
import { unwrap } from '@/lib/queryKeys'
import type { SlashCommand } from '@/lib/slashCommands'

const NONE: SlashCommand[] = []

/**
 * The slash commands the chat's tool offers in this project: its skills and
 * custom commands on disk, plus built-ins it announced while running. Kept
 * fresh for half a minute, so a skill installed while GitWyrm is open shows
 * up on the next keystroke after that, and a tool's built-ins appear once it
 * has run once.
 */
export function useSlashCommands(provider: string | null, repoPath: string | null | undefined): SlashCommand[] {
  const query = useQuery({
    queryKey: ['slashCommands', provider ?? 'default', repoPath ?? ''] as const,
    queryFn: async () => unwrap(await commands.agentSlashCommands(repoPath ?? null, provider)),
    staleTime: 30_000,
  })
  return query.data ?? NONE
}
