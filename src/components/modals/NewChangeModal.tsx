import { useEffect, useMemo, useState } from 'react'
import { Check, FilePlus2, Loader2, Sparkles } from 'lucide-react'
import { toast } from 'sonner'
import { Input } from '@/components/ui/input'
import { Textarea } from '@/components/ui/textarea'
import { FormDialog } from '@/components/ui/form-dialog'
import { cn } from '@/lib/utils'
import type { DraftedArtifact, DraftedChange } from '@/lib/bindings'
import { useOpenspecChanges, useOpenspecMutations } from '@/hooks/useOpenspec'
import { useSpecAi } from '@/hooks/useSpecAi'
import { useAiSelection } from '@/hooks/useAiSelection'
import { selectChangeEverywhere } from '@/lib/specSync'
import { describeError } from '@/lib/log'
import { useUiStore } from '@/stores/uiStore'
import { useActiveRepo } from '@/stores/workspaceStore'

/**
 * Preview of the folder name a title will become.
 *
 * Mirrors `sanitize_change_id` in src-tauri/src/openspec/write.rs, which is the
 * source of truth -- this only exists so the user sees the real name before
 * committing to it. The backend sanitizes again regardless, so a drift here
 * costs a surprising preview, never a wrong folder.
 */
function previewId(raw: string): string {
  const out: string[] = []
  let lastDash = true
  for (const ch of raw.trim()) {
    const c = ch.toLowerCase()
    if (/[a-z0-9]/.test(c)) {
      out.push(c)
      lastDash = false
    } else if (!lastDash) {
      out.push('-')
      lastDash = true
    }
  }
  return out.join('').replace(/-+$/, '')
}

/**
 * The stages drafting moves through, in order.
 *
 * Shown as a checklist that fills in rather than a spinner: drafting takes tens
 * of seconds, and a spinner that long reads as a hang. The stages are also the
 * honest answer to "what is the AI reading?", which is why the first one names
 * the specs library explicitly.
 */
const STAGES = [
  'Reading your specs library',
  'Drafting the proposal',
  'Breaking it into tasks',
  'Writing spec deltas',
] as const

/** How long each stage is displayed before the next lights up. */
const STAGE_MS = 2200

type Phase = 'form' | 'drafting' | 'review'

/** Which drafted artifacts the user is keeping. */
interface Kept {
  proposal: boolean
  tasks: boolean
  deltas: boolean
}

/**
 * @param repoId Repository to create the change in. Omitted in the main window,
 * which reads the active repo from the store. The Spec Desk must pass its own:
 * that window never runs launch restore, so its store has no open repos and no
 * active one -- `useActiveRepo()` there is always null, and creating a change
 * would reach the backend with no repository at all.
 */
export function NewChangeModal({ repoId }: { repoId?: string }) {
  const open = useUiStore((s) => s.activeModal === 'newChange')
  const closeModal = useUiStore((s) => s.closeModal)

  const activeRepo = useActiveRepo()
  const targetRepoId = repoId ?? activeRepo?.id ?? null
  const changes = useOpenspecChanges(targetRepoId)
  const { scaffoldChange, draftChange, createDraftedChange } =
    useOpenspecMutations(targetRepoId)
  const ai = useSpecAi()
  const selection = useAiSelection()

  const [title, setTitle] = useState('')
  const [why, setWhy] = useState('')
  const [phase, setPhase] = useState<Phase>('form')
  const [stage, setStage] = useState(0)
  const [draft, setDraft] = useState<DraftedChange | null>(null)
  const [kept, setKept] = useState<Kept>({ proposal: true, tasks: true, deltas: true })
  // Bumped whenever the dialog closes or reopens. The draft request cannot be
  // aborted mid-flight, so its result is matched against this on arrival and
  // dropped if it belongs to a run the user already walked away from --
  // otherwise cancelling then reopening would snap to a stale review screen.
  const [runId, setRunId] = useState(0)

  useEffect(() => {
    setRunId((n) => n + 1)
    if (open) {
      setTitle('')
      setWhy('')
      setPhase('form')
      setStage(0)
      setDraft(null)
      setKept({ proposal: true, tasks: true, deltas: true })
    }
  }, [open])

  // The stage checklist advances on a timer rather than on real backend events:
  // the model returns one reply at the end, so there is nothing finer-grained to
  // report. It stops at the last stage instead of looping, so a slow draft shows
  // "writing spec deltas" rather than restarting and implying a retry.
  useEffect(() => {
    if (phase !== 'drafting') return
    const timer = setInterval(() => {
      setStage((s) => Math.min(s + 1, STAGES.length - 1))
    }, STAGE_MS)
    return () => clearInterval(timer)
  }, [phase])

  const id = previewId(title)
  const taken = useMemo(
    () => new Set((changes.data ?? []).map((c) => c.id)),
    [changes.data]
  )

  const error =
    title.trim() === ''
      ? null
      : id === ''
        ? 'Use at least one letter or number.'
        : taken.has(id)
          ? `There is already a change named ${id}.`
          : null

  const canCreate = targetRepoId != null && id !== '' && !error && !scaffoldChange.isPending

  const create = () => {
    if (!canCreate) return
    scaffoldChange.mutate(
      { name: title.trim(), description: why.trim() },
      {
        onSuccess: (result) => {
          // Select it so the card and the Desk land on what was just made.
          selectChangeEverywhere(result.id)
          closeModal()
          toast.success(`Started ${result.id}.`, {
            description: 'Write the proposal and add tasks in the Spec Desk.',
          })
        },
        onError: (e) => toast.error(String(e)),
      }
    )
  }

  // Drafting needs something to describe. The title alone is a name, not a
  // brief, so the description carries whichever the user actually filled in.
  const brief = why.trim() || title.trim()
  // `ai.configured` already means a provider, a model, and live entitlements,
  // so this is belt-and-braces for the two values actually sent.
  const canDraft =
    ai.configured &&
    selection.provider != null &&
    selection.model != null &&
    targetRepoId != null &&
    id !== '' &&
    !error

  const startDraft = () => {
    if (!canDraft || !selection.provider || !selection.model) return
    const startedAs = runId
    setPhase('drafting')
    setStage(0)
    draftChange.mutate(
      {
        name: title.trim(),
        description: brief,
        provider: selection.provider,
        model: selection.model,
      },
      {
        onSuccess: (result) => {
          // The user cancelled (or reopened) while this was in flight. Drop it:
          // nothing was written, so there is nothing to clean up.
          if (startedAs !== runId) return
          setDraft(result)
          setPhase('review')
        },
        onError: (e) => {
          if (startedAs !== runId) return
          // Back to the form with everything still typed: a failed draft should
          // cost a retry, not the user's words.
          setPhase('form')
          toast.error('Could not draft this change.', { description: describeError(e) })
        },
      }
    )
  }

  /** Exactly what Create would write, in the order it writes it. */
  const keptArtifacts: DraftedArtifact[] = useMemo(() => {
    if (!draft) return []
    const out: DraftedArtifact[] = []
    if (kept.proposal) out.push(draft.proposal)
    if (kept.tasks) out.push(draft.tasks)
    if (kept.deltas) out.push(...draft.deltas)
    return out
  }, [draft, kept])

  const createDrafted = () => {
    if (!draft || keptArtifacts.length === 0) return
    createDraftedChange.mutate(
      { id: draft.id, artifacts: keptArtifacts },
      {
        onSuccess: (result) => {
          selectChangeEverywhere(result.id)
          closeModal()
          toast.success(`Created ${result.id}.`, {
            description: `Wrote ${result.files.length} file${result.files.length === 1 ? '' : 's'}. Review it in the Spec Desk.`,
          })
        },
        onError: (e) => toast.error('Could not create this change.', {
          description: describeError(e),
        }),
      }
    )
  }

  if (phase === 'drafting') {
    return (
      <FormDialog
        open={open}
        onOpenChange={(o) => !o && closeModal()}
        icon={<Sparkles size={15} strokeWidth={1.9} />}
        title="Drafting your change"
        submitLabel="Drafting…"
        canSubmit={false}
        // Deliberately not `pending`: FormDialog blocks Escape and outside
        // clicks while pending, and this is the one phase the user must always
        // be able to leave. Nothing has been written, so leaving costs nothing.
        cancelLabel="Cancel"
        onSubmit={() => {}}
      >
        <p className="text-2xs leading-relaxed text-muted-foreground">
          {ai.providerShort} is reading your specs library, your recent commits, and what
          you wrote. Nothing is saved until you review it.
        </p>
        <ul className="grid gap-1.5">
          {STAGES.map((text, i) => (
            <li key={text} className="flex items-center gap-2 text-2xs">
              {i < stage ? (
                <Check size={12} className="flex-none text-accent-text" strokeWidth={2.6} />
              ) : i === stage ? (
                <Loader2 size={12} className="flex-none animate-spin text-accent-text" />
              ) : (
                <span className="size-3 flex-none" aria-hidden />
              )}
              <span className={cn(i <= stage ? 'text-foreground' : 'text-muted-foreground')}>
                {text}
              </span>
            </li>
          ))}
        </ul>
        <p className="text-2xs text-muted-foreground">
          Cancel stops this and writes nothing.
        </p>
      </FormDialog>
    )
  }

  if (phase === 'review' && draft) {
    const count = keptArtifacts.length
    return (
      <FormDialog
        open={open}
        onOpenChange={(o) => !o && closeModal()}
        icon={<Sparkles size={15} strokeWidth={1.9} />}
        // Wider than a normal form: this body is three file previews, and
        // wrapping markdown into a narrow column makes it unreviewable.
        widthClassName="sm:max-w-2xl"
        title={`Review ${draft.id}`}
        submitLabel={count === 0 ? 'Nothing kept' : `Create (writes ${count} file${count === 1 ? '' : 's'})`}
        pendingLabel="Creating…"
        canSubmit={count > 0 && !createDraftedChange.isPending}
        pending={createDraftedChange.isPending}
        cancelLabel="Discard"
        onSubmit={createDrafted}
      >
        {draft.renamed && (
          <p className="rounded-md border border-[var(--gw-amber)]/40 bg-[var(--gw-amber)]/8 px-3 py-2 text-2xs leading-relaxed text-[var(--gw-amber)]">
            That name was taken, so this change will be called{' '}
            <span className="font-mono">{draft.id}</span>.
          </p>
        )}
        <p className="text-2xs leading-relaxed text-muted-foreground">
          Nothing is on disk yet. Keep what you want and press Create; skipped parts are
          never written.
        </p>

        <ArtifactCard
          label="Proposal"
          path={draft.proposal.path}
          content={draft.proposal.content}
          kept={kept.proposal}
          onToggle={() => setKept((k) => ({ ...k, proposal: !k.proposal }))}
        />
        <ArtifactCard
          label="Tasks"
          path={draft.tasks.path}
          content={draft.tasks.content}
          kept={kept.tasks}
          onToggle={() => setKept((k) => ({ ...k, tasks: !k.tasks }))}
        />
        {draft.deltas.length > 0 && (
          <ArtifactCard
            label={`Spec deltas (${draft.deltas.length})`}
            path={draft.deltas.map((d) => d.path).join(', ')}
            content={draft.deltas.map((d) => d.content).join('\n\n')}
            kept={kept.deltas}
            onToggle={() => setKept((k) => ({ ...k, deltas: !k.deltas }))}
          />
        )}
        {draft.deltas.length === 0 && (
          <p className="text-2xs text-muted-foreground">
            No spec deltas: the AI judged this change adds no new requirements. You can
            add them later.
          </p>
        )}
        {count === 0 && (
          <p className="text-2xs text-[var(--gw-amber)]">
            Keep at least one part to create this change.
          </p>
        )}
      </FormDialog>
    )
  }

  return (
    <FormDialog
      open={open}
      onOpenChange={(o) => !o && closeModal()}
      icon={<FilePlus2 size={15} strokeWidth={1.9} />}
      title="Start a new change"
      submitLabel="Start blank"
      pendingLabel="Creating…"
      canSubmit={canCreate}
      pending={scaffoldChange.isPending}
      onSubmit={create}
      footerExtra={
        // "Draft it for me" is the primary path when an AI is set up, but it
        // sits beside Start blank rather than replacing it: a blank change is
        // still the right answer when you already know what you want to write.
        ai.configured ? (
          <button
            type="button"
            onClick={startDraft}
            disabled={!canDraft}
            className="mr-auto flex items-center gap-1.5 rounded-md bg-primary px-3 py-1.5 text-2xs font-semibold text-primary-foreground transition-colors hover:brightness-110 disabled:opacity-50"
          >
            <Sparkles size={12} strokeWidth={2.4} />
            Draft it for me
          </button>
        ) : undefined
      }
    >
      <div className="grid gap-1.5">
        <label className="text-2xs font-semibold text-sub">What are you changing?</label>
        <Input
          value={title}
          onChange={(e) => setTitle(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === 'Enter') create()
          }}
          placeholder="Add dark mode to the diff view"
          className="h-auto bg-background py-1.5 text-xs"
          autoFocus
        />
        <p className="min-h-[15px] text-2xs leading-tight">
          {error ? (
            <span className="text-removed">{error}</span>
          ) : id ? (
            <span className="text-muted-foreground">
              Saved as <span className="font-mono text-sub">openspec/changes/{id}</span>
            </span>
          ) : (
            ''
          )}
        </p>
      </div>

      <div className="grid gap-1.5">
        <label className="text-2xs font-semibold text-sub">
          Why does it matter?{' '}
          <span className="font-normal text-muted-foreground">
            {ai.configured ? '(the AI drafts from this)' : '(optional)'}
          </span>
        </label>
        <Textarea
          value={why}
          onChange={(e) => setWhy(e.target.value)}
          placeholder="The problem this solves."
          rows={3}
          className="bg-background px-2.5 py-2 text-xs"
        />
      </div>

      <p className="text-2xs text-muted-foreground">
        {ai.configured
          ? `Start blank creates an empty proposal and task list. Draft it for me has ${ai.providerShort} write them first, for you to review before anything is saved.`
          : 'Creates a proposal and an empty task list. You fill them in from the Spec Desk.'}
      </p>
    </FormDialog>
  )
}

/**
 * One drafted artifact, with the exact file body it would write.
 *
 * The preview is the real content rather than a summary: "review before write"
 * is only true if what is shown is what lands on disk.
 */
function ArtifactCard({
  label,
  path,
  content,
  kept,
  onToggle,
}: {
  label: string
  path: string
  content: string
  kept: boolean
  onToggle: () => void
}) {
  return (
    <section
      className={cn(
        'rounded-lg border px-3 py-2.5 transition-colors',
        kept ? 'border-primary/35 bg-soft' : 'border-border bg-panel2 opacity-60'
      )}
    >
      <header className="flex items-center gap-2">
        <div className="min-w-0 flex-1">
          <h3 className="text-2xs font-bold tracking-[.08em] text-sub">{label}</h3>
          <p className="truncate font-mono text-2xs text-muted-foreground">{path}</p>
        </div>
        <button
          type="button"
          onClick={onToggle}
          className={cn(
            'flex-none rounded border px-2 py-0.5 text-2xs font-semibold transition-colors',
            kept
              ? 'border-primary/40 text-accent-text hover:bg-primary/15'
              : 'border-border text-muted-foreground hover:text-sub'
          )}
        >
          {kept ? 'Keep' : 'Skipped'}
        </button>
      </header>
      <pre className="mt-2 max-h-40 overflow-auto whitespace-pre-wrap rounded border border-border bg-background px-2.5 py-2 font-mono text-2xs leading-relaxed text-sub">
        {content}
      </pre>
    </section>
  )
}
