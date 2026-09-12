import { useState } from 'react'
import { ChevronDown, FolderGit2, FolderOpen, Link2 } from 'lucide-react'
import type { SessionSource } from '@/lib/bindings'
import { sourceKindLabel } from '@/lib/agentSessionGrouping'
import { adapterDisplayName } from '@/lib/agentImportDisplay'
import { cn } from '@/lib/utils'
import { DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuTrigger } from '@/components/ui/dropdown-menu'

export interface ChatProjectChoice {
  path: string
  name: string
}

/** One thing worth starting from, drawn from the repository's real state. */
export interface ChatStarter {
  /** Stable key. */
  id: string
  /** The button's words. Plain, and about the user's files. */
  label: string
  /** What lands in the message box when pressed. */
  prompt: string
}

/**
 * "1 file", never "1 file(s)".
 *
 * Every other place that counts files -- `SessionContextPanel` included, which
 * describes these same two sources -- writes it properly, so this was the odd
 * one out, and the shorthand sat on the first screen a new person meets.
 */
function fileCount(n: number): string {
  return `${n} file${n === 1 ? '' : 's'}`
}

/**
 * What started this chat, in one line, or `null` when nothing did.
 *
 * Deliberately not `SessionSourceBanner`'s function of the same name: that one
 * returns a three-part kicker/title/meta for a full-width banner row and has no
 * answer for a manual chat, because the banner is never drawn for one. This
 * returns a title and a detail, and answers `null` for manual -- which is what
 * a chip needs, and what this file's own tests check.
 */
export function describeSource(source: SessionSource | null): { title: string; detail: string | null } | null {
  if (!source || source.kind === 'manual') return null
  const kind = sourceKindLabel(source.kind)
  switch (source.kind) {
    case 'issue':
    case 'pullRequest':
      return {
        title: source.snapshot.title || `${kind} #${source.number}`,
        detail: `${kind} #${source.number} in ${source.owner}/${source.repo}`,
      }
    case 'openSpecChange':
      return { title: source.snapshot.title || source.changeId, detail: `${kind}: ${source.changeId}` }
    case 'openSpecTask':
      return { title: source.taskText || source.snapshot.title || kind, detail: `${kind} in ${source.changeId}` }
    case 'commit':
      return { title: source.snapshot.title || kind, detail: `${kind} ${source.oid.slice(0, 8)}` }
    case 'diff':
      return { title: source.snapshot.title || kind, detail: `${fileCount(source.paths.length)} changed` }
    case 'workingChanges':
      return {
        title: source.snapshot.title || kind,
        detail: `${fileCount(source.paths.length)} not yet committed`,
      }
    case 'checkFailure':
      return { title: source.snapshot.title || kind, detail: `${kind} from ${source.provider}` }
    case 'imported':
      return { title: source.snapshot.title || kind, detail: `From ${adapterDisplayName(source.adapterId)}` }
  }
}

/**
 * What a chat with nothing in it yet shows.
 *
 * Three things: which project, what you want, and -- only when the repository
 * actually offers one -- somewhere to start. Nothing else.
 *
 * This used to be a five-section form. Project, AI, "how much can it do?" and
 * "how many agents?" each got a labelled heading and a row of described cards,
 * and then the composer twelve inches below offered the same four decisions
 * again as chips. Measured: 34 controls and ~79 words of instructions before
 * anyone could type a character, with three decisions rendered twice in one
 * viewport.
 *
 * The composer won, for a reason worth keeping written down: this panel
 * disappears the moment the first message lands, so every control taught here
 * is a control the user then loses. The chips persist. Teaching the permanent
 * UI is the only version that pays back.
 *
 * What remains is the part no other agent client can show -- the repository
 * this chat is bound to, and work that is really sitting in it.
 */
export function NewChatLanding({
  projectName,
  projectPath,
  projects,
  onProjectChange,
  onProjectPathChosen,
  projectChanging = false,
  starters,
  onStarterPick,
  source,
}: {
  projectName: string
  projectPath: string
  projects: ChatProjectChoice[]
  onProjectChange: (project: ChatProjectChoice) => void
  /** A folder picked from disk rather than from the list. */
  onProjectPathChosen: (path: string) => void
  projectChanging?: boolean
  /**
   * Starting points built from what is actually in the repository.
   *
   * Empty is a perfectly good answer and renders nothing. A row of invented
   * suggestions would be the generic version of this idea; the whole value is
   * that each one names real work.
   */
  starters: ChatStarter[]
  onStarterPick: (starter: ChatStarter) => void
  /** What started this chat. `null` while the session is still loading. */
  source: SessionSource | null
}) {
  const [browsing, setBrowsing] = useState(false)
  const startedFrom = describeSource(source)

  const browseForFolder = async () => {
    if (browsing) return
    setBrowsing(true)
    try {
      // Lazily imported, as every other folder picker in the app does it, so
      // the dialog plugin stays out of the initial bundle.
      const { open } = await import('@tauri-apps/plugin-dialog')
      const picked = await open({ directory: true, multiple: false, title: 'Open a folder' })
      if (typeof picked === 'string') onProjectPathChosen(picked)
    } finally {
      setBrowsing(false)
    }
  }

  return (
    // `min-h-0` and its own scrollbar: a flex child defaults to
    // `min-height: auto` and refuses to shrink below its content, which used
    // to push the pane header and the top of the sidebar off screen. Centred
    // by auto margins rather than `justify-center`, because centring content
    // taller than its box overflows the top edge too, and a scrollbar cannot
    // reach it.
    <div className="flex min-h-0 flex-1 flex-col items-center gap-5 overflow-y-auto px-6 py-8">
      <div className="mt-auto flex w-full max-w-lg flex-col items-center gap-3">
        <h2 className="text-center text-xl font-semibold tracking-tight text-foreground">
          What are we working on in <span className="text-accent-text">{projectName}</span>?
        </h2>

        {/* The project, as one chip. It was a full-width card with the whole
            Windows path underneath -- the machine detail this product exists
            to spare people. The path is the tooltip, for the moment somebody
            genuinely needs it. */}
        <div className="flex items-center gap-1.5">
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <button
                type="button"
                // Opening a project arms a watcher over the whole tree, which
                // can take seconds. Without this the click did nothing
                // visible until it finished.
                disabled={projectChanging}
                title={projectPath}
                // The visible chip is the project name alone, which read as a
                // bare word with no hint that it opens anything. The path
                // lives in `title`, which a keyboard or screen-reader user
                // never reaches, so it is named here too -- the one place the
                // machine detail is genuinely wanted is when you are checking
                // which of two similarly named folders this chat is bound to.
                aria-label={`Project: ${projectName}${projectPath ? ` (${projectPath})` : ''}. Choose a different project`}
                className="flex items-center gap-1.5 rounded-full border border-border px-2.5 py-1 text-2xs font-semibold text-sub hover:bg-panel3 hover:text-foreground disabled:cursor-not-allowed disabled:opacity-60"
              >
                <FolderGit2 size={12} className="flex-none text-accent-text" aria-hidden />
                <span className="max-w-[16rem] truncate">{projectChanging ? 'Opening…' : projectName}</span>
                <ChevronDown size={11} className="flex-none" aria-hidden />
              </button>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="center" className="w-[min(26rem,calc(100vw-3rem))]">
              {projects.map((project) => (
                <DropdownMenuItem key={project.path} onSelect={() => onProjectChange(project)}>
                  <FolderGit2 />
                  <span className="min-w-0 flex-1 truncate">{project.name}</span>
                </DropdownMenuItem>
              ))}
              {/* Any folder on disk, not only the ones already open. A chat
                  about a parent directory of several repositories is a real
                  thing to want, and a list of known projects cannot say it. */}
              <DropdownMenuItem onSelect={() => void browseForFolder()}>
                <FolderOpen />
                <span className="min-w-0 flex-1 truncate">{browsing ? 'Choosing…' : 'Open another folder…'}</span>
              </DropdownMenuItem>
            </DropdownMenuContent>
          </DropdownMenu>

          {startedFrom && (
            <span
              className="flex items-center gap-1.5 rounded-full border border-border px-2.5 py-1 text-2xs font-semibold text-sub"
              title={startedFrom.detail ?? startedFrom.title}
              // The visible text truncates at 18rem and the rest of the
              // sentence is in `title`, which never reaches a screen reader.
              // Read whole, it is the one thing on this screen no other agent
              // client can say: what this chat was started from.
              aria-label={
                startedFrom.detail
                  ? `Started from ${startedFrom.title} -- ${startedFrom.detail}`
                  : `Started from ${startedFrom.title}`
              }
            >
              <Link2 size={12} className="flex-none text-accent-text" aria-hidden />
              <span className="max-w-[18rem] truncate" aria-hidden>
                {startedFrom.title}
              </span>
            </span>
          )}
        </div>
      </div>

      {/* Only ever what the repository really offers. No starters is the
          ordinary case for a clean tree and renders nothing at all, rather
          than a row of prompts anyone could have written. */}
      {starters.length > 0 ? (
        // Named as a group, so a screen reader reaches a labelled set of
        // optional starting points rather than a bare run of buttons with no
        // hint of what they are or that skipping them is fine. The composer's
        // own control rows are grouped the same way.
        <div
          role="group"
          aria-label="Starting points from this project"
          className="mb-auto flex w-full max-w-lg flex-wrap justify-center gap-1.5"
        >
          {starters.map((starter) => (
            <button
              key={starter.id}
              type="button"
              onClick={() => onStarterPick(starter)}
              className={cn(
                'rounded-full border border-border px-3 py-1 text-2xs font-medium text-sub',
                'hover:border-primary/50 hover:bg-soft hover:text-foreground'
              )}
            >
              {starter.label}
            </button>
          ))}
        </div>
      ) : (
        <div className="mb-auto" />
      )}
    </div>
  )
}
