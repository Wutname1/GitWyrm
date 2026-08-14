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

export function UpdateModalPreview() {
  const params = new URLSearchParams(window.location.search)
  const state = (params.get('state') ?? 'available') as
    | 'available'
    | 'downloading'
    | 'downloaded'
    | 'ready'
  const empty = params.get('empty') === '1'
  const loading = params.get('loading') === '1'

  return (
    <UpdateModalView
      open
      onClose={() => {}}
      state={state}
      version="0.9.0"
      progress={
        state === 'downloading' ? { downloaded: 42_300_000, total: 96_100_000 } : null
      }
      changelog={empty ? [] : FIXTURE}
      loading={loading}
      autoRestart={false}
      onAutoRestartChange={() => {}}
      onDownload={() => {}}
      onRestart={() => {}}
    />
  )
}
