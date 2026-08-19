/**
 * Standalone harness for the update modal.
 *
 * The full app cannot run in a plain browser -- it calls Tauri commands during
 * boot -- so this mounts the modal's presentational pieces on their own against
 * fixture data. Reached at /?preview=update-modal under `npm run dev`.
 *
 * Dev-only: nothing imports this outside main.tsx's preview branch, so it is
 * tree-shaken out of a production build.
 */
import { UpdateModalView } from '@/components/domain/UpdateModal'
import type { ChangelogEntry } from '@/lib/bindings'

const FIXTURE: ChangelogEntry[] = [
  {
    version: '0.8.0',
    released_at: '2026-08-13T09:27:53Z',
    items: [
      {
        section: 'feature',
        text: 'The installer fetches git and gpg from a tool manifest',
        tags: ['updater'],
      },
      {
        section: 'feature',
        text: 'Connect GitLab, Bitbucket, and Azure DevOps as well as GitHub',
        tags: ['remotes', 'github'],
      },
      {
        section: 'fix',
        text: 'The update window no longer vanishes the moment it opens',
        tags: ['updater'],
      },
      {
        section: 'fix',
        text: 'Signed commits no longer fail with "Invalid user ID"',
        tags: ['commits'],
      },
      {
        section: 'change',
        text: 'Wire perf trail into diagnostics and UI',
        tags: ['performance'],
      },
    ],
  },
  {
    version: '0.7.0',
    released_at: '2026-07-30T11:02:00Z',
    items: [
      {
        section: 'breaking',
        text: 'Older profile files are migrated on first launch',
        tags: ['settings'],
      },
      { section: 'feature', text: 'Stash entries appear in the graph', tags: ['stash'] },
      { section: 'docs', text: 'Document the commit message convention', tags: [] },
    ],
  },
  {
    version: '0.6.1',
    released_at: '2026-07-12T08:00:00Z',
    items: [
      { section: 'fix', text: 'Submodule status no longer flickers', tags: ['submodules'] },
    ],
  },
]

/**
 * A release long enough to overflow the dialog.
 *
 * The short fixture above cannot reproduce layout bugs that only appear once
 * the notes are taller than the viewport, which is the case that matters --
 * a real 0.9.0 release ran to ~30 items. Reached with `?long=1`.
 */
const LONG_FIXTURE: ChangelogEntry[] = [
  {
    version: '0.9.0',
    released_at: '2026-08-16T09:00:00Z',
    items: Array.from({ length: 30 }, (_, i) => ({
      section: (['feature', 'fix', 'change'] as const)[i % 3],
      text: `Changelog line ${i + 1} of a release with a great many entries in it`,
      tags: i % 2 === 0 ? ['updater'] : [],
    })),
  },
  ...FIXTURE,
]

export function UpdateModalPreview() {
  const params = new URLSearchParams(window.location.search)
  const long = params.get('long') === '1'
  // `?state=manual` shows a deb/rpm install owned by the package manager.
  const state = (params.get('state') ?? 'available') as
    | 'available'
    | 'downloading'
    | 'downloaded'
    | 'ready'
    | 'manual'
  const empty = params.get('empty') === '1'
  const loading = params.get('loading') === '1'

  return (
    <UpdateModalView
      open
      onClose={() => {}}
      state={state}
      version="0.9.0"
      // `?current=` exercises the header's fallback: an empty value stands in
      // for a version that could not be read.
      currentVersion={params.get('current') ?? '0.6.1'}
      progress={
        state === 'downloading' ? { downloaded: 42_300_000, total: 96_100_000 } : null
      }
      manualUrl={
        state === 'manual' ? 'https://docs.gitwyrm.com/guide/install-linux' : null
      }
      changelog={empty ? [] : long ? LONG_FIXTURE : FIXTURE}
      loading={loading}
      autoRestart={false}
      onAutoRestartChange={() => {}}
      onDownload={() => {}}
      onRestart={() => {}}
    />
  )
}
