import { useQuery } from '@tanstack/react-query'
import { open as openDialog } from '@tauri-apps/plugin-dialog'
import { AtSign, Blocks, FileText, Folder, Plug, Plus, Slash } from 'lucide-react'
import { commands, type ClientId } from '@/lib/bindings'
import { keys, unwrap } from '@/lib/queryKeys'
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuShortcut,
  DropdownMenuSub,
  DropdownMenuSubContent,
  DropdownMenuSubTrigger,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu'
import { useAgentDeskUiStore } from '@/stores/agentDeskUiStore'

/** Which agent-config client holds each tool's connectors, when GitWyrm can read them. */
const CONNECTOR_CLIENT: Record<string, ClientId | undefined> = {
  claude: 'claude-code',
  codex: 'codex',
  opencode: 'open-code',
}

/** Sync states that mean "this tool has the connector". */
const PRESENT = new Set(['same', 'different', 'outdated', 'isSource', 'keptSeparate'])

/**
 * The "+" beside the message box: attach files or a folder, mention a project
 * file, start a slash command, and see the connectors and plugins the chat's
 * tool has.
 *
 * Connectors and plugins are listed to browse, not toggled here: each tool
 * keeps them in its own settings, and Agent setup is where GitWyrm changes
 * those, with a preview and an undo.
 */
export function ComposerAddMenu({
  sessionId,
  repoId,
  provider,
  onAddPaths,
  onStartMention,
  onStartSlash,
}: {
  sessionId: string | null
  repoId: string | null
  /** The chat's chosen tool, or `null` for the default one. */
  provider: string | null
  onAddPaths: (paths: string[]) => void
  onStartMention: () => void
  onStartSlash: () => void
}) {
  const setCenterView = useAgentDeskUiStore((s) => s.setCenterView)
  const providers = useQuery({
    queryKey: keys.agentProviders(sessionId),
    queryFn: async () => unwrap(await commands.agentProvidersList(sessionId)),
  })
  const rows = providers.data?.providers ?? []
  const tool = rows.find((r) => r.id === provider) ?? rows.find((r) => r.isDefault)
  const toolId = tool?.id ?? null
  const client = toolId ? CONNECTOR_CLIENT[toolId] : undefined

  const connectors = useQuery({
    queryKey: ['agentConfigScan', repoId] as const,
    queryFn: async () => unwrap(await commands.agentConfigScan(repoId)),
    enabled: client != null,
    staleTime: 60_000,
  })
  const toolConnectors = (connectors.data ?? []).filter(
    (e) => e.kind === 'mcpConnector' && e.perClient.some((c) => c.client === client && PRESENT.has(c.state))
  )

  const plugins = useQuery({
    queryKey: ['agentPlugins', toolId] as const,
    queryFn: async () => unwrap(await commands.agentPlugins(toolId)),
    enabled: toolId === 'claude',
    staleTime: 60_000,
  })

  const pickFiles = async () => {
    const picked = await openDialog({ multiple: true, title: 'Add files to this message' })
    const paths = Array.isArray(picked) ? picked : picked ? [picked] : []
    if (paths.length > 0) onAddPaths(paths)
  }
  const pickFolder = async () => {
    const picked = await openDialog({ directory: true, title: 'Add a folder to this message' })
    if (typeof picked === 'string') onAddPaths([picked])
  }

  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <button
          type="button"
          aria-label="Add to this message"
          title="Add files, a folder, a project file, or a command"
          className="flex h-6 w-6 flex-none items-center justify-center rounded text-muted-foreground hover:bg-panel3 hover:text-foreground"
        >
          <Plus size={14} aria-hidden />
        </button>
      </DropdownMenuTrigger>
      <DropdownMenuContent
        side="top"
        align="start"
        className="w-60"
        // "Project file" and "Slash commands" put the caret in the message box.
        // Left to itself the menu hands focus back to this button as it
        // closes, and the next letters typed went nowhere.
        onCloseAutoFocus={(e) => e.preventDefault()}
      >
        <DropdownMenuItem onSelect={() => void pickFiles()}>
          <FileText aria-hidden />
          Add files…
        </DropdownMenuItem>
        <DropdownMenuItem onSelect={() => void pickFolder()}>
          <Folder aria-hidden />
          Add folder…
        </DropdownMenuItem>
        <DropdownMenuSeparator />
        <DropdownMenuItem onSelect={onStartMention}>
          <AtSign aria-hidden />
          Project file…
          <DropdownMenuShortcut>@</DropdownMenuShortcut>
        </DropdownMenuItem>
        <DropdownMenuItem onSelect={onStartSlash}>
          <Slash aria-hidden />
          Slash commands
          <DropdownMenuShortcut>/</DropdownMenuShortcut>
        </DropdownMenuItem>
        <DropdownMenuSeparator />

        <DropdownMenuSub>
          <DropdownMenuSubTrigger>
            <Plug aria-hidden />
            Connectors
          </DropdownMenuSubTrigger>
          <DropdownMenuSubContent className="w-60">
            <DropdownMenuLabel className="text-2xs text-muted-foreground">
              {tool ? `${tool.displayName} can use` : 'Connectors'}
            </DropdownMenuLabel>
            {client == null ? (
              <p className="px-2 pb-1.5 text-2xs leading-relaxed text-muted-foreground">
                GitWyrm cannot read this tool's connectors yet. Its own settings list them.
              </p>
            ) : connectors.isLoading ? (
              <p className="px-2 pb-1.5 text-2xs text-muted-foreground">Looking…</p>
            ) : connectors.isError ? (
              <p className="px-2 pb-1.5 text-2xs leading-relaxed text-[var(--gw-amber)]">
                Could not read this tool's connectors just now.
              </p>
            ) : toolConnectors.length === 0 ? (
              <p className="px-2 pb-1.5 text-2xs text-muted-foreground">No connectors set up.</p>
            ) : (
              toolConnectors.map((c) => (
                <DropdownMenuItem key={c.itemId} onSelect={() => setCenterView('setup')}>
                  <span className="min-w-0 flex-1 truncate">{c.displayName}</span>
                </DropdownMenuItem>
              ))
            )}
            <DropdownMenuSeparator />
            <DropdownMenuItem onSelect={() => setCenterView('setup')}>Manage connectors…</DropdownMenuItem>
          </DropdownMenuSubContent>
        </DropdownMenuSub>

        <DropdownMenuSub>
          <DropdownMenuSubTrigger>
            <Blocks aria-hidden />
            Plugins
          </DropdownMenuSubTrigger>
          <DropdownMenuSubContent className="w-60">
            {toolId !== 'claude' ? (
              <p className="px-2 py-1.5 text-2xs leading-relaxed text-muted-foreground">
                {tool ? `${tool.displayName} has no plugins GitWyrm can read.` : 'No plugins found.'}
              </p>
            ) : plugins.isLoading ? (
              <p className="px-2 py-1.5 text-2xs text-muted-foreground">Looking…</p>
            ) : plugins.isError ? (
              <p className="px-2 py-1.5 text-2xs leading-relaxed text-[var(--gw-amber)]">
                Could not read Claude Code's plugins just now.
              </p>
            ) : (plugins.data ?? []).length === 0 ? (
              <p className="px-2 py-1.5 text-2xs text-muted-foreground">No plugins installed.</p>
            ) : (
              // A list to read, not buttons: switching a plugin on or off
              // belongs to Claude Code's own settings.
              <ul className="px-2 py-1">
                {(plugins.data ?? []).map((p) => (
                  <li key={`${p.name}@${p.marketplace ?? ''}`} className="flex items-baseline gap-2 py-0.5 text-xs">
                    <span className="min-w-0 flex-1 truncate text-foreground">{p.name}</span>
                    <span className="flex-none text-2xs text-muted-foreground">{p.enabled ? 'On' : 'Off'}</span>
                  </li>
                ))}
              </ul>
            )}
          </DropdownMenuSubContent>
        </DropdownMenuSub>
      </DropdownMenuContent>
    </DropdownMenu>
  )
}
