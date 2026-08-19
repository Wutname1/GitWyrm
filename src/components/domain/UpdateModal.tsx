import { useMemo } from 'react'
import { ArrowRight, Download, ExternalLink, Loader2, RefreshCw } from 'lucide-react'
// The unified `radix-ui` package, matching components/ui/dialog.tsx.
// `@radix-ui/react-dialog` resolves here only as a transitive install and is
// not a declared dependency.
import { Dialog as DialogPrimitive } from 'radix-ui'

import {
  useUpdater,
  type DownloadProgress,
  type UpdateState,
} from '@/hooks/useUpdater'
import { useWorkspaceStore } from '@/stores/workspaceStore'
import { Button } from '@/components/ui/button'
import { ScrollArea } from '@/components/ui/scroll-area'
import { cn } from '@/lib/utils'
import type { ChangelogEntry, ChangelogItem } from '@/lib/bindings'

const GITHUB_RELEASES = 'https://github.com/Wutname1/GitWyrm/releases'

/**
 * How each commit-prefix section is labelled and coloured.
 *
 * Keyed by the `section` slug the changelog API returns, which comes from the
 * commit-prefix convention (new:/fixes:/improved:). An unknown slug falls back
 * to a neutral chip rather than being dropped -- a new section type should show
 * up looking plain, not vanish.
 */
const SECTIONS: Record<string, { label: string; className: string }> = {
  feature: {
    label: 'New',
    className: 'bg-[color-mix(in_srgb,var(--gw-accent)_18%,transparent)] text-accent-text',
  },
  fix: {
    label: 'Fixed',
    className: 'bg-[color-mix(in_srgb,var(--gw-red)_16%,transparent)] text-[var(--gw-red)]',
  },
  change: {
    label: 'Improved',
    className: 'bg-[color-mix(in_srgb,var(--gw-blue)_16%,transparent)] text-[var(--gw-blue)]',
  },
  breaking: {
    label: 'Breaking',
    className: 'bg-[color-mix(in_srgb,var(--gw-amber)_18%,transparent)] text-[var(--gw-amber)]',
  },
  docs: {
    label: 'Docs',
    className: 'bg-[color-mix(in_srgb,var(--gw-purple)_16%,transparent)] text-[var(--gw-purple)]',
  },
}

/** Order sections appear in, most interesting first. */
const SECTION_ORDER = ['breaking', 'feature', 'fix', 'change', 'docs']

function sectionRank(slug: string): number {
  const i = SECTION_ORDER.indexOf(slug)
  // Unknown sections sort last rather than interleaving unpredictably.
  return i === -1 ? SECTION_ORDER.length : i
}

/** "Aug 13, 2026", or nothing when the date is missing or unparseable. */
function formatDate(iso: string | null): string | null {
  if (!iso) return null
  const d = new Date(iso)
  if (Number.isNaN(d.getTime())) return null
  return d.toLocaleDateString(undefined, {
    year: 'numeric',
    month: 'short',
    day: 'numeric',
  })
}

/** "4.2 MB of 96.1 MB" */
function formatBytes(n: number): string {
  return `${(n / 1_000_000).toFixed(1)} MB`
}

function SectionChip({ slug }: { slug: string }) {
  const section = SECTIONS[slug]
  return (
    <span
      className={cn(
        'shrink-0 rounded px-1.5 py-0.5 font-sans text-2xs font-semibold',
        section?.className ?? 'bg-panel3 text-sub',
      )}
    >
      {section?.label ?? slug}
    </span>
  )
}

/** One changelog line: its section chip, the text, then any author tags. */
function Item({ item }: { item: ChangelogItem }) {
  return (
    <li className="flex items-start gap-2 py-1">
      <SectionChip slug={item.section} />
      <span className="min-w-0 flex-1 text-xs leading-relaxed text-text">
        {item.text}
        {item.tags && item.tags.length > 0 && (
          <span className="ml-1.5 inline-flex flex-wrap gap-1 align-middle">
            {item.tags.map((tag) => (
              <span
                key={tag}
                className="rounded-sm bg-panel3 px-1 py-px font-mono text-2xs text-muted-foreground"
              >
                {tag}
              </span>
            ))}
          </span>
        )}
      </span>
    </li>
  )
}

/** One release: its version heading, date, and grouped items. */
function Release({ entry, isTarget }: { entry: ChangelogEntry; isTarget: boolean }) {
  // Group by section so a release reads as "what's new / what's fixed" rather
  // than the commit order it happened to be written in.
  const grouped = useMemo(() => {
    const bySection = new Map<string, ChangelogItem[]>()
    for (const item of entry.items ?? []) {
      const list = bySection.get(item.section)
      if (list) list.push(item)
      else bySection.set(item.section, [item])
    }
    return [...bySection.entries()].sort((a, b) => sectionRank(a[0]) - sectionRank(b[0]))
  }, [entry.items])

  const date = formatDate(entry.released_at)

  return (
    <section className="border-b border-border/60 px-4 py-3 last:border-b-0">
      <header className="mb-1.5 flex items-baseline gap-2">
        <span
          className={cn(
            'rounded px-1.5 py-0.5 font-mono text-2xs font-semibold',
            isTarget
              ? 'bg-[color-mix(in_srgb,var(--gw-accent)_20%,transparent)] text-accent-text'
              : 'bg-panel3 text-sub',
          )}
        >
          v{entry.version}
        </span>
        {date && <span className="text-2xs text-muted-foreground">{date}</span>}
      </header>

      {grouped.length === 0 ? (
        <p className="text-xs text-muted-foreground">No release notes for this version.</p>
      ) : (
        <ul className="space-y-px">
          {grouped.flatMap(([, items]) =>
            items.map((item, i) => <Item key={`${item.section}-${i}`} item={item} />),
          )}
        </ul>
      )}
    </section>
  )
}

/**
 * The update details modal: what changed, and the two-step download/restart.
 *
 * Shows notes for EVERY version newer than the running build, not just the
 * target. Someone going 0.3.0 -> 0.5.0 skipped 0.4.x entirely, and this is the
 * only place those notes are ever surfaced.
 */
export function UpdateModal() {
  const open = useUpdater((s) => s.modalOpen)
  const closeModal = useUpdater((s) => s.closeModal)
  const state = useUpdater((s) => s.state)
  const version = useUpdater((s) => s.version)
  const currentVersion = useUpdater((s) => s.currentVersion)
  const progress = useUpdater((s) => s.progress)
  const manualUrl = useUpdater((s) => s.manualUrl)
  const changelog = useUpdater((s) => s.changelog)
  const loading = useUpdater((s) => s.changelogLoading)
  const download = useUpdater((s) => s.download)
  const restartAndInstall = useUpdater((s) => s.restartAndInstall)

  const autoRestart = useWorkspaceStore((s) => s.autoRestartAfterDownload)
  const setAutoRestart = useWorkspaceStore((s) => s.setAutoRestartAfterDownload)

  return (
    <UpdateModalView
      open={open}
      onClose={closeModal}
      state={state}
      version={version}
      currentVersion={currentVersion}
      progress={progress}
      manualUrl={manualUrl}
      changelog={changelog}
      loading={loading}
      autoRestart={autoRestart}
      onAutoRestartChange={setAutoRestart}
      onDownload={() => void download()}
      onRestart={() => void restartAndInstall()}
    />
  )
}

export interface UpdateModalViewProps {
  open: boolean
  onClose: () => void
  state: UpdateState
  version: string | null
  currentVersion: string | null
  progress: DownloadProgress | null
  /** Where to download by hand, when the app cannot install the update itself. */
  manualUrl: string | null
  changelog: ChangelogEntry[]
  loading: boolean
  autoRestart: boolean
  onAutoRestartChange: (next: boolean) => void
  onDownload: () => void
  onRestart: () => void
}

/**
 * The modal's presentation, with no store access.
 *
 * Split out so the visuals can be rendered against fixtures: the full app calls
 * Tauri commands while booting and so cannot run in a plain browser, which
 * would otherwise leave this component impossible to look at outside a release
 * build.
 */
export function UpdateModalView({
  open,
  onClose,
  state,
  version,
  currentVersion,
  progress,
  manualUrl,
  changelog,
  loading,
  autoRestart,
  onAutoRestartChange,
  onDownload,
  onRestart,
}: UpdateModalViewProps) {
  const downloading = state === 'downloading'
  const downloaded = state === 'downloaded'
  const restarting = state === 'ready'
  // A Linux .deb or .rpm belongs to apt/dnf. Nothing to retry inside the app,
  // so the button opens the package-manager instructions instead.
  const manual = state === 'manual'

  const fraction =
    progress && progress.total && progress.total > 0
      ? progress.downloaded / progress.total
      : null

  return (
    <DialogPrimitive.Root open={open} onOpenChange={(next) => !next && onClose()}>
      <DialogPrimitive.Portal>
        {/* Blur here rather than in the shared DialogOverlay: this is the one
            dialog that sits over the whole app as an announcement, and blurring
            every dialog in the app would be a much wider change. */}
        <DialogPrimitive.Overlay className="fixed inset-0 z-50 bg-black/60 backdrop-blur-sm duration-200 data-[state=closed]:animate-out data-[state=closed]:fade-out-0 data-[state=open]:animate-in data-[state=open]:fade-in-0" />

        <DialogPrimitive.Content
          // max-h-[80vh] with a flex column: the header and footer keep their
          // height and the notes list is what scrolls, so a 19-release
          // changelog cannot push the buttons off screen.
          className="fixed left-1/2 top-1/2 z-50 flex max-h-[80vh] w-[min(42rem,calc(100vw-2rem))] -translate-x-1/2 -translate-y-1/2 flex-col overflow-hidden rounded-lg border border-border bg-panel2 shadow-2xl duration-200 data-[state=closed]:animate-out data-[state=closed]:fade-out-0 data-[state=closed]:zoom-out-95 data-[state=open]:animate-in data-[state=open]:fade-in-0 data-[state=open]:zoom-in-95"
        >
          <header className="flex shrink-0 items-center gap-2 border-b border-border px-4 py-3">
            <Download className="size-4 text-accent-text" />
            <DialogPrimitive.Title className="font-sans text-sm font-semibold text-text">
              Update available
            </DialogPrimitive.Title>
            {/* Both ends of the jump, so the size of the change is obvious --
                0.8.0 to 0.9.0 reads very differently from 0.3.0 to 0.9.0, and
                the target alone hid that. Falls back to the target on its own
                if the running version could not be read. */}
            {version && (
              <span className="flex items-baseline gap-1.5 font-mono text-xs">
                {currentVersion && (
                  <>
                    <span className="text-muted-foreground">{currentVersion}</span>
                    <ArrowRight className="size-3 self-center text-muted-foreground" />
                  </>
                )}
                <span className="text-accent-text">{version}</span>
              </span>
            )}
            <DialogPrimitive.Description className="sr-only">
              Release notes for the versions newer than the one you are running.
            </DialogPrimitive.Description>
          </header>

          <ScrollArea className="min-h-0 flex-1">
            {loading ? (
              <div className="flex items-center justify-center gap-2 py-10 text-xs text-sub">
                <Loader2 className="size-3.5 animate-spin" />
                Loading release notes
              </div>
            ) : changelog.length === 0 ? (
              <p className="px-4 py-8 text-center text-xs text-muted-foreground">
                Release notes could not be loaded. The update is still safe to install.
              </p>
            ) : (
              changelog.map((entry, i) => (
                <Release key={entry.version} entry={entry} isTarget={i === 0} />
              ))
            )}
          </ScrollArea>

          {/* Download progress spans the full width, between notes and actions,
              so it reads as belonging to the whole dialog rather than to the
              button that started it. */}
          {downloading && (
            <div className="shrink-0 border-t border-border px-4 py-2">
              <div className="mb-1 flex items-center justify-between text-2xs text-sub">
                <span>Downloading</span>
                {progress && (
                  <span className="font-mono">
                    {formatBytes(progress.downloaded)}
                    {progress.total ? ` of ${formatBytes(progress.total)}` : ''}
                  </span>
                )}
              </div>
              <div className="h-1 overflow-hidden rounded-full bg-panel3">
                <div
                  className={cn(
                    'h-full rounded-full bg-accent-text',
                    // No Content-Length: shuttle rather than sit at zero, which
                    // would read as a stalled download.
                    fraction === null && 'w-1/3 animate-pulse',
                  )}
                  style={fraction === null ? undefined : { width: `${fraction * 100}%` }}
                />
              </div>
            </div>
          )}

          <footer className="flex shrink-0 items-center gap-3 border-t border-border px-4 py-3">
            <a
              href={GITHUB_RELEASES}
              target="_blank"
              rel="noreferrer"
              className="flex items-center gap-1.5 text-2xs text-sub transition-colors hover:text-text"
            >
              <ExternalLink className="size-3" />
              GitHub
            </a>

            {manual ? (
              <p className="ml-auto max-w-[22rem] text-right text-2xs text-sub">
                This copy was installed as a Linux package. Update it with your
                package manager to keep system updates in one place.
              </p>
            ) : (
              <label className="ml-auto flex cursor-pointer items-center gap-1.5 text-2xs text-sub">
                <input
                  type="checkbox"
                  checked={autoRestart}
                  onChange={(e) => onAutoRestartChange(e.target.checked)}
                  className="size-3.5 accent-[var(--gw-accent)]"
                />
                Restart automatically
              </label>
            )}

            {manual ? (
              <Button size="sm" asChild>
                <a href={manualUrl ?? GITHUB_RELEASES} target="_blank" rel="noreferrer">
                  <ExternalLink className="size-3.5" />
                  View update steps
                </a>
              </Button>
            ) : downloaded || restarting ? (
              <Button size="sm" onClick={onRestart} disabled={restarting}>
                {restarting ? (
                  <>
                    <Loader2 className="size-3.5 animate-spin" />
                    Restarting
                  </>
                ) : (
                  <>
                    <RefreshCw className="size-3.5" />
                    Restart to update
                  </>
                )}
              </Button>
            ) : (
              <Button size="sm" onClick={onDownload} disabled={downloading}>
                {downloading ? (
                  <>
                    <Loader2 className="size-3.5 animate-spin" />
                    Downloading
                  </>
                ) : (
                  <>
                    <Download className="size-3.5" />
                    Download
                  </>
                )}
              </Button>
            )}
          </footer>
        </DialogPrimitive.Content>
      </DialogPrimitive.Portal>
    </DialogPrimitive.Root>
  )
}
