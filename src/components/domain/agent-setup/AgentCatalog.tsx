import { useState } from 'react'
import { useQuery } from '@tanstack/react-query'
import { AlertTriangle, Bot, Check, ExternalLink, RefreshCw } from 'lucide-react'
import { commands } from '@/lib/bindings'
import type { AgentProvider } from '@/lib/bindings'
import { keys, unwrap } from '@/lib/queryKeys'
import { cn } from '@/lib/utils'
import { useWorkspaceStore } from '@/stores/workspaceStore'

/**
 * Which AI tools GitWyrm can drive, and which of them are on this machine.
 *
 * Two buckets from one list: the tools that are here, and the same rows dimmed
 * and demoted for the ones that are not. Deliberately not a separate "browse"
 * screen — a tool you do not have is the same thing as a tool you do, minus
 * the parts that need a binary.
 *
 * The point of the second bucket is that it is not a dead end. Every row
 * carries somewhere to go and the command that installs it, so "not installed"
 * is the start of a two-minute fix rather than the end of the conversation.
 *
 * Colour follows one rule: green means done, amber means unfinished, and red
 * is kept for the check itself failing. A tool you have not installed is not
 * an error.
 */
export function AgentCatalog() {
  const query = useQuery({
    queryKey: keys.agentProviders(null),
    queryFn: async () => unwrap(await commands.agentProvidersList(null)),
  })
  const [refreshing, setRefreshing] = useState(false)
  // Every hook must run on every render, so these live above the early
  // returns below. They were added underneath them, which made the hook
  // count jump from two to four the moment the query settled -- React throws
  // "Rendered more hooks than during the previous render" on that transition,
  // which is every ordinary open of this tab, and the app-level boundary
  // replaced the whole window with the crash screen.
  const defaultAgentTool = useWorkspaceStore((st) => st.defaultAgentTool)
  const setDefaultAgentTool = useWorkspaceStore((st) => st.setDefaultAgentTool)

  async function refresh() {
    setRefreshing(true)
    try {
      // The refresh command re-reads the system PATH before looking again,
      // which is the whole reason this button works after an install. See
      // `ai::agent::shell_path`.
      const fresh = unwrap(await commands.agentProvidersRefresh(null))
      query.refetch()
      return fresh
    } finally {
      setRefreshing(false)
    }
  }

  // Three states, not two. Never asked, asked and found none, and asked and
  // found some all need different words.
  if (query.isLoading) {
    return (
      <p className="rounded-md border border-dashed border-border py-6 text-center text-2xs text-muted-foreground">
        Looking for AI tools on this machine…
      </p>
    )
  }

  if (query.isError) {
    // The only red on this screen: the check itself did not run. Missing
    // software is never this.
    return (
      <div className="rounded-md border border-destructive/40 bg-destructive/10 p-3">
        <p className="flex items-center gap-1.5 text-2xs font-semibold text-destructive">
          <AlertTriangle size={13} aria-hidden />
          GitWyrm could not check which AI tools you have.
        </p>
        <button
          type="button"
          onClick={() => void refresh()}
          disabled={query.isFetching}
          className="mt-2 rounded border border-border px-2 py-1 text-2xs font-semibold hover:bg-panel3 disabled:cursor-not-allowed disabled:opacity-60"
        >
          {query.isFetching ? 'Checking…' : 'Try again'}
        </button>
      </div>
    )
  }

  const rows = query.data?.providers ?? []
  const installed = rows.filter((r) => r.installed)
  const missing = rows.filter((r) => !r.installed)

  return (
    <div className="flex flex-col gap-5">
      {installed.length > 0 && (
        // The default a new chat follows. Before this there was a per-chat
        // override with nothing to override FROM: someone with two tools
        // installed re-picked in every chat, and the app had no way to be
        // told which one they wanted.
        <section className="flex flex-col gap-1.5 rounded-md border border-border bg-panel2 p-2.5">
          <div>
            <h3 className="text-2xs font-semibold text-foreground">Which tool new chats use</h3>
            <p className="mt-0.5 text-2xs leading-relaxed text-muted-foreground">
              A chat can still pick a different one for itself. This is only the starting point.
            </p>
          </div>
          <select
            value={defaultAgentTool}
            onChange={(e) => setDefaultAgentTool(e.target.value)}
            aria-label="Which AI tool new chats use"
            className="h-7 w-full rounded border border-border bg-panel px-1.5 text-2xs text-foreground"
          >
            <option value="">Let GitWyrm choose</option>
            {installed.map((row) => (
              <option key={row.id} value={row.id}>
                {row.displayName || row.id}
              </option>
            ))}
          </select>
        </section>
      )}

      <section className="flex flex-col gap-2">
        <header className="flex items-center gap-2">
          <h3 className="text-2xs font-semibold text-foreground">Installed</h3>
          <span className="rounded-full bg-soft px-1.5 py-0.5 font-mono text-2xs text-accent-text">
            {installed.length} found
          </span>
          <button
            type="button"
            onClick={() => void refresh()}
            disabled={refreshing}
            title="Read your system PATH again and look for AI tools"
            className="ml-auto flex items-center gap-1 rounded px-1.5 py-0.5 text-2xs font-semibold text-sub hover:bg-panel3 hover:text-foreground disabled:opacity-50"
          >
            <RefreshCw size={11} className={cn(refreshing && 'animate-spin')} aria-hidden />
            {refreshing ? 'Checking…' : 'Refresh'}
          </button>
        </header>

        {installed.length === 0 ? (
          <p className="rounded-md border border-dashed border-border px-3 py-4 text-center text-2xs leading-relaxed text-muted-foreground">
            None of the AI tools GitWyrm knows how to use are on this machine yet. Pick one below,
            install it, then press Refresh.
          </p>
        ) : (
          <ul className="flex flex-col gap-1.5">
            {installed.map((row) => (
              <AgentRow key={row.id} row={row} />
            ))}
          </ul>
        )}
      </section>

      {missing.length > 0 && (
        <section className="flex flex-col gap-2">
          <header className="flex items-center gap-2">
            <h3 className="text-2xs font-semibold text-muted-foreground">Available to install</h3>
            <span className="rounded-full border border-border px-1.5 py-0.5 font-mono text-2xs text-muted-foreground">
              {missing.length}
            </span>
          </header>
          <ul className="flex flex-col gap-1.5">
            {missing.map((row) => (
              <AgentRow key={row.id} row={row} />
            ))}
          </ul>
        </section>
      )}
    </div>
  )
}

function AgentRow({ row }: { row: AgentProvider }) {
  return (
    <li
      className={cn(
        'flex items-start gap-2.5 rounded-md border border-border px-3 py-2.5',
        !row.installed && 'opacity-70'
      )}
    >
      <span className="mt-0.5 flex size-6 flex-none items-center justify-center rounded border border-border">
        <Bot size={13} className="text-muted-foreground" aria-hidden />
      </span>

      <div className="flex min-w-0 flex-1 flex-col gap-0.5">
        <span className="flex items-center gap-1.5">
          <span className="text-xs font-medium text-foreground">{row.displayName}</span>
          {row.isDefault && (
            <span className="rounded bg-soft px-1 py-px font-mono text-2xs text-accent-text">
              default
            </span>
          )}
          {row.tooOld && (
            <span className="rounded border border-amber-500/40 bg-amber-500/10 px-1 py-px font-mono text-2xs text-amber-600 dark:text-amber-300">
              too old
            </span>
          )}
        </span>

        {/* The binary being looked for, on every row including missing ones.
            A package name, a binary name and a product name are routinely three
            different strings; when detection is wrong this line explains why. */}
        <span className="truncate font-mono text-2xs text-muted-foreground">
          {row.installed && row.version ? row.version : row.binaryName}
        </span>

        {!row.installed && (
          <span className="mt-1 flex flex-wrap items-center gap-1.5">
            <code className="rounded bg-panel3 px-1.5 py-0.5 font-mono text-2xs text-foreground">
              {row.installHint}
            </code>
            <span className="text-2xs text-muted-foreground">then press Refresh</span>
          </span>
        )}

        {row.tooOld && (
          <span className="mt-0.5 text-2xs leading-snug text-muted-foreground">
            Updating it is enough — GitWyrm found it, but this version is older than it can drive.
          </span>
        )}
      </div>

      {/* One control, both states. Its wording changes; the destination does
          not need to. */}
      <a
        href={row.homepageUrl}
        target="_blank"
        rel="noopener noreferrer"
        title={row.installed ? `${row.displayName} documentation` : `How to install ${row.displayName}`}
        aria-label={row.installed ? `${row.displayName} documentation` : `How to install ${row.displayName}`}
        className="mt-0.5 flex size-6 flex-none items-center justify-center rounded text-muted-foreground hover:bg-panel3 hover:text-foreground"
      >
        <ExternalLink size={12} aria-hidden />
      </a>

      {row.installed && (
        <Check size={14} className="mt-1 flex-none text-[var(--gw-green)]" aria-hidden />
      )}
    </li>
  )
}
