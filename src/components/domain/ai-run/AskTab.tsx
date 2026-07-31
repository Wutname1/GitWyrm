import { useState } from 'react'
import { BookOpen, Eye, PencilLine, Play, Send } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { cn } from '@/lib/utils'
import type { AskSource } from '@/lib/bindings'
import { commands } from '@/lib/bindings'
import { unwrap } from '@/lib/queryKeys'
import { describeError } from '@/lib/log'
import { useAskStore, type AskTurn } from '@/stores/askStore'
import { useAiSelection } from '@/hooks/useAiSelection'
import { useSpecAi } from '@/hooks/useSpecAi'
import { detectEditRequest } from '@/lib/specEditRequest'

/**
 * The ✦ tab in ask mode: a read-only conversation about one change.
 *
 * Deliberately unlike the run tab. A run is a step stream with a state pill and
 * a Stop button; this is chat bubbles with a standing read-only banner and
 * neither of those. Mode confusion -- "I thought it was just answering, why did
 * files change?" -- is the worst failure this feature could have, so the two
 * modes are made to look different on purpose rather than sharing a chrome.
 */
export function AskTab({
  repoId,
  changeId,
  onOpenTab,
  onStartRun,
  onDraftEdit,
  deltaFiles,
}: {
  repoId: string
  changeId: string
  /** Jump to a Desk tab when a source chip is clicked. */
  onOpenTab: (tab: string) => void
  /**
   * Offered when the user asks for work rather than an explanation. Absent when
   * there is no task left to run, so the escalation is never a dead button.
   */
  onStartRun?: () => void
  /**
   * Offered when the user asks for a spec file to be reworded. Drafting writes
   * nothing: it returns a proposed file the Desk shows as a diff.
   */
  onDraftEdit?: (file: string, instruction: string) => void
  /** The change's delta paths, so a question naming one matches a real file. */
  deltaFiles?: readonly string[]
}) {
  const [draft, setDraft] = useState('')
  const ai = useSpecAi()
  const selection = useAiSelection()
  const session = useAskStore((s) => s.byRepo[repoId])
  const epoch = useAskStore((s) => s.epoch)
  const addQuestion = useAskStore((s) => s.ask)
  const settle = useAskStore((s) => s.settle)

  const pending = session?.pending ?? false
  const turns = session?.turns ?? []

  const send = async () => {
    const text = draft.trim()
    if (!text || pending || !selection.provider || !selection.model) return
    setDraft('')
    const turnId = addQuestion(repoId, text)
    if (turnId < 0) return
    const startedAt = epoch
    try {
      const answer = unwrap(
        await commands.openspecAsk(
          repoId,
          changeId,
          text,
          selection.provider,
          selection.model
        )
      )
      settle(repoId, turnId, startedAt, { answer })
    } catch (e) {
      settle(repoId, turnId, startedAt, { error: describeError(e) })
    }
  }

  return (
    <div className="flex h-full min-h-0 flex-col">
      {/* The banner is persistent, not a one-time notice: it is the thing that
          keeps a glance at this tab unambiguous. */}
      <div className="flex flex-none items-center gap-2 border-b border-border bg-panel2 px-3 py-2">
        <Eye size={12} strokeWidth={2.2} className="flex-none text-accent-text" />
        <p className="min-w-0 flex-1 text-2xs leading-snug text-sub">
          Read-only — the AI reads this change and the code, but changes nothing.
        </p>
        {ai.configured && (
          <span className="flex-none text-2xs text-muted-foreground">
            {ai.providerShort} · read-only
          </span>
        )}
      </div>

      <div className="min-h-0 flex-1 overflow-y-auto px-3 py-3">
        {turns.length === 0 && (
          <div className="px-1 py-6 text-center">
            <BookOpen size={18} className="mx-auto text-muted-foreground" strokeWidth={1.8} />
            <p className="mt-2 text-2xs leading-relaxed text-muted-foreground">
              Ask why this change is built the way it is, what a task means, or which
              files it touches. Nothing you ask here can change anything.
            </p>
          </div>
        )}
        {turns.map((turn) => (
          <Turn
            key={turn.id}
            turn={turn}
            onOpenTab={onOpenTab}
            onStartRun={onStartRun}
            // Worked out from the question the user typed, not from the answer:
            // the model is told it cannot edit, so its reply will not volunteer
            // an offer to. The intent is in the question either way.
            editRequest={detectEditRequest(turn.question, deltaFiles)}
            onDraftEdit={onDraftEdit}
          />
        ))}
        {pending && (
          <p className="px-1 py-2 text-2xs text-muted-foreground">Reading this change…</p>
        )}
      </div>

      <div className="flex flex-none items-center gap-2 border-t border-border px-3 py-2">
        <Input
          value={draft}
          onChange={(e) => setDraft(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === 'Enter' && !e.shiftKey) {
              e.preventDefault()
              void send()
            }
          }}
          placeholder="Ask about this change"
          disabled={pending}
          className="h-auto bg-background py-1.5 text-xs"
        />
        <Button
          size="sm"
          disabled={pending || !draft.trim()}
          onClick={() => void send()}
          className="h-7 flex-none"
        >
          <Send size={12} strokeWidth={2.2} />
        </Button>
      </div>
    </div>
  )
}

/** One question and its answer. */
function Turn({
  turn,
  onOpenTab,
  onStartRun,
  editRequest,
  onDraftEdit,
}: {
  turn: AskTurn
  onOpenTab: (tab: string) => void
  onStartRun?: () => void
  editRequest: { file: string; instruction: string } | null
  onDraftEdit?: (file: string, instruction: string) => void
}) {
  return (
    <div className="mb-3 last:mb-0">
      <div className="flex justify-end">
        <p className="max-w-[85%] rounded-lg rounded-br-sm bg-primary px-2.5 py-1.5 text-2xs leading-relaxed text-primary-foreground">
          {turn.question}
        </p>
      </div>
      {turn.error && (
        <p className="mt-1.5 max-w-[85%] rounded-lg rounded-bl-sm border border-[var(--gw-red)]/40 bg-[var(--gw-red)]/8 px-2.5 py-1.5 text-2xs leading-relaxed text-removed">
          {turn.error}
        </p>
      )}
      {turn.answer && (
        <div className="mt-1.5 max-w-[85%]">
          <p className="whitespace-pre-wrap rounded-lg rounded-bl-sm border border-border bg-panel2 px-2.5 py-1.5 text-2xs leading-relaxed text-foreground">
            {turn.answer.text}
          </p>
          {turn.answer.sources.length > 0 && (
            <div className="mt-1.5 flex flex-wrap gap-1">
              {turn.answer.sources.map((source: AskSource) => (
                <button
                  key={source.label}
                  type="button"
                  onClick={() => onOpenTab(source.tab)}
                  className="rounded border border-primary/40 bg-soft px-1.5 py-px font-mono text-2xs text-accent-text hover:bg-primary/15"
                >
                  {source.label}
                </button>
              ))}
            </div>
          )}
          {/* Asked for a file to be reworded: offer to draft it. Drafting still
              writes nothing -- it produces a diff to look at -- so this stays
              inside the read-only promise the banner makes. */}
          {editRequest && onDraftEdit && (
            <button
              type="button"
              onClick={() => onDraftEdit(editRequest.file, editRequest.instruction)}
              className={cn(
                'mt-2 flex items-center gap-1.5 rounded-md border border-primary/50 bg-soft px-2.5 py-1',
                'text-2xs font-semibold text-accent-text transition-colors hover:bg-primary/15'
              )}
            >
              <PencilLine size={11} strokeWidth={2.4} />
              Draft this edit to {editRequest.file}
            </button>
          )}
          {/* Promotion from read-only to write is always one visible click. The
              button is offered; it never fires on its own. */}
          {turn.answer.wants_run && onStartRun && (
            <button
              type="button"
              onClick={onStartRun}
              className={cn(
                'mt-2 flex items-center gap-1.5 rounded-md bg-primary px-2.5 py-1 text-2xs font-semibold',
                'text-primary-foreground transition-colors hover:brightness-110'
              )}
            >
              <Play size={11} strokeWidth={2.6} />
              Start a run with AI
            </button>
          )}
        </div>
      )}
    </div>
  )
}
