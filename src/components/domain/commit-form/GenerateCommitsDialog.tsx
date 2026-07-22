import { useEffect, useMemo, useRef, useState } from 'react'
import { listen } from '@tauri-apps/api/event'
import {
  Brain,
  Check,
  CircleAlert,
  FileSearch,
  GitCommitHorizontal,
  Layers3,
  Loader2,
  Minus,
  Plus,
  ShieldCheck,
  Sparkles,
} from 'lucide-react'
import { toast } from 'sonner'
import logoUrl from '@/assets/logo.png'
import { Button } from '@/components/ui/button'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import { Textarea } from '@/components/ui/textarea'
import { useAiConfigured, useAiMutations } from '@/hooks/useAi'
import type { AiCreatedCommit } from '@/lib/bindings'
import { describeError, log } from '@/lib/log'
import { useActiveRepo, useWorkspaceStore } from '@/stores/workspaceStore'

type ProgressKind = 'scan' | 'plan' | 'check' | 'stage' | 'commit' | 'done' | 'error'

interface AiCommitProgressPayload {
  repo_id: string
  kind: ProgressKind
  message: string
  detail: string
}

const progressIcons: Record<ProgressKind, typeof Sparkles> = {
  scan: FileSearch,
  plan: Brain,
  check: ShieldCheck,
  stage: Layers3,
  commit: GitCommitHorizontal,
  done: Check,
  error: CircleAlert,
}

interface GenerateCommitsDialogProps {
  changedFiles: number
  hasConflicts: boolean
}

function shortSha(sha: string) {
  return sha.slice(0, 7)
}

export function GenerateCommitsDialog({
  changedFiles,
  hasConflicts,
}: GenerateCommitsDialogProps) {
  const repo = useActiveRepo()
  const configured = useAiConfigured()
  const ai = useAiMutations()
  const aiProvider = useWorkspaceStore((state) => state.aiProvider)
  const aiModel = useWorkspaceStore((state) => state.aiModel)
  const aiReady =
    aiProvider != null &&
    aiModel != null &&
    (configured.data ?? []).some((provider) => provider.id === aiProvider)

  const maxCommits = Math.min(8, Math.max(2, changedFiles))
  const defaultCount = Math.min(maxCommits, changedFiles >= 8 ? 3 : 2)
  const [open, setOpen] = useState(false)
  const [count, setCount] = useState(defaultCount)
  const [instructions, setInstructions] = useState('')
  const [created, setCreated] = useState<AiCreatedCommit[] | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [activity, setActivity] = useState<AiCommitProgressPayload[]>([])
  const [startedAt, setStartedAt] = useState<number | null>(null)
  const [elapsedSeconds, setElapsedSeconds] = useState(0)
  const activityRef = useRef<HTMLDivElement>(null)
  const pending = ai.generateCommits.isPending
  const working = pending && error == null

  useEffect(() => {
    if (!repo) return
    const repoId = repo.id
    const unlisten = listen<AiCommitProgressPayload>('ai-commit-progress', (event) => {
      if (event.payload.repo_id !== repoId) return
      if (event.payload.kind === 'error') setError(event.payload.detail)
      setActivity((current) => {
        const previous = current.at(-1)
        if (
          previous?.kind === event.payload.kind &&
          previous.message === event.payload.message &&
          previous.detail === event.payload.detail
        ) {
          return current
        }
        return [...current, event.payload]
      })
    })
    return () => {
      unlisten.then((dispose) => dispose())
    }
  }, [repo])

  useEffect(() => {
    if (!working || startedAt == null) return
    const updateElapsed = () => setElapsedSeconds(Math.floor((Date.now() - startedAt) / 1000))
    updateElapsed()
    const timer = window.setInterval(updateElapsed, 1000)
    return () => window.clearInterval(timer)
  }, [working, startedAt])

  useEffect(() => {
    const viewport = activityRef.current
    if (viewport) viewport.scrollTop = viewport.scrollHeight
  }, [activity])

  const fileLabel = useMemo(
    () => `${changedFiles} changed file${changedFiles === 1 ? '' : 's'}`,
    [changedFiles]
  )

  if (!aiReady || changedFiles === 0) return null

  const changeOpen = (next: boolean) => {
    if (!next && working) return
    setOpen(next)
    if (next) {
      setCount(defaultCount)
      setInstructions('')
      setCreated(null)
      setError(null)
      setActivity([])
      setStartedAt(null)
      setElapsedSeconds(0)
    }
  }

  const generate = () => {
    if (!repo || pending || hasConflicts) return
    setError(null)
    setCreated(null)
    setStartedAt(Date.now())
    setElapsedSeconds(0)
    setActivity([
      {
        repo_id: repo.id,
        kind: 'scan',
        message: `Starting ${count}-commit run`,
        detail: `Preparing to organize ${fileLabel}.`,
      },
    ])
    ai.generateCommits.mutate(
      {
        repoId: repo.id,
        provider: aiProvider!,
        model: aiModel!,
        commitCount: count,
        specialInstructions: instructions.trim(),
      },
      {
        onSuccess: (commits) => {
          setCreated(commits)
          toast.success(`Created ${commits.length} commits`)
        },
        onError: (reason) => {
          const message = reason instanceof Error ? reason.message : String(reason)
          log.error(`AI commit generation failed: ${describeError(reason)}`)
          setError(message)
          setActivity((current) => {
            const failure: AiCommitProgressPayload = {
              repo_id: repo.id,
              kind: 'error',
              message: 'Commit generation stopped',
              detail: message,
            }
            const previous = current.at(-1)
            return previous?.kind === 'error' && previous.detail === message
              ? current
              : [...current, failure]
          })
        },
      }
    )
  }

  return (
    <Dialog open={open} onOpenChange={changeOpen}>
      <Button
        size="xs"
        onClick={() => changeOpen(true)}
        className="h-6 border border-primary/45 bg-soft px-2 text-2xs font-semibold text-accent-text hover:border-primary hover:bg-primary hover:text-primary-foreground"
      >
        <Sparkles />
        Generate commits
      </Button>

      <DialogContent
        className="gap-0 overflow-hidden p-0 sm:max-w-[34rem]"
        showCloseButton={!working}
        onEscapeKeyDown={(event) => working && event.preventDefault()}
        onPointerDownOutside={(event) => working && event.preventDefault()}
        aria-describedby="generate-commits-description"
      >
        <DialogHeader className="border-b border-border px-5 pb-4 pt-5">
          <DialogTitle className="flex items-center gap-2.5 text-base">
            <span className="grid size-8 place-items-center rounded-md border border-primary/35 bg-soft text-accent-text">
              <Sparkles size={16} />
            </span>
            {created ? `${created.length} commits created` : 'Generate commits'}
          </DialogTitle>
          <DialogDescription id="generate-commits-description" className="pl-[42px] text-xs">
            {created
              ? 'Every change is included. Your files stayed in place, and nothing was pushed.'
              : `AI will organize ${fileLabel} and create the commits for you.`}
          </DialogDescription>
        </DialogHeader>

        {created ? (
          <div className="max-h-[25rem] overflow-y-auto px-5 py-5">
            <div className="relative grid gap-2.5 before:absolute before:bottom-5 before:left-[17px] before:top-5 before:w-px before:bg-primary/25">
              {created.map((commit, index) => (
                <div
                  key={commit.sha}
                  className="relative grid grid-cols-[35px_minmax(0,1fr)] gap-3 rounded-lg border border-border bg-panel2 px-3 py-3"
                >
                  <span className="relative z-[1] grid size-[35px] place-items-center rounded-full border border-primary/40 bg-background font-mono text-2xs font-bold text-accent-text">
                    {index + 1}
                  </span>
                  <div className="min-w-0 pt-0.5">
                    <div className="truncate text-xs font-semibold text-foreground">
                      {commit.summary}
                    </div>
                    {commit.description && (
                      <div className="mt-1 line-clamp-2 text-2xs leading-relaxed text-sub">
                        {commit.description}
                      </div>
                    )}
                    <div className="mt-2 flex items-center gap-2 font-mono text-2xs text-muted-foreground">
                      <span>{shortSha(commit.sha)}</span>
                      <span>·</span>
                      <span>
                        {commit.files.length} file{commit.files.length === 1 ? '' : 's'}
                      </span>
                    </div>
                  </div>
                </div>
              ))}
            </div>
          </div>
        ) : pending || error ? (
          <div className="grid min-h-[19rem] grid-rows-[auto_minmax(0,1fr)] gap-4 px-5 py-5">
            <div className="flex items-center gap-3 rounded-lg border border-primary/30 bg-soft px-3.5 py-3">
              <span className="grid size-11 flex-none place-items-center rounded-full border border-primary/30 bg-background">
                {working ? (
                  <img
                    src={logoUrl}
                    alt=""
                    className="wyrm-ai-logo wyrm-ai-logo-compact object-contain"
                  />
                ) : (
                  <CircleAlert size={18} className="text-removed" />
                )}
              </span>
              <span className="min-w-0 flex-1">
                <span className="block text-xs font-semibold text-foreground">
                  {working ? `Building ${count} commits` : 'Stopped before finishing'}
                </span>
                <span className="mt-0.5 block font-mono text-2xs text-muted-foreground">
                  {working ? `Working for ${elapsedSeconds}s` : `Stopped after ${elapsedSeconds}s`}
                </span>
              </span>
              {working && <Loader2 size={16} className="animate-spin text-accent-text" />}
            </div>

            <div
              ref={activityRef}
              className="max-h-[18rem] overflow-y-auto rounded-lg border border-border bg-panel2"
              aria-live="polite"
              aria-label="Commit generation activity"
            >
              <div className="relative grid gap-0 px-3 py-2 before:absolute before:bottom-5 before:left-[26px] before:top-5 before:w-px before:bg-border">
                {activity.map((item, index) => {
                  const Icon = progressIcons[item.kind]
                  const active = working && index === activity.length - 1
                  const failed = item.kind === 'error'
                  return (
                    <div
                      key={`${index}-${item.message}`}
                      className="relative grid grid-cols-[29px_minmax(0,1fr)] gap-3 px-1 py-2.5"
                    >
                      <span
                        className={`relative z-[1] grid size-[29px] place-items-center rounded-full border bg-background ${
                          failed
                            ? 'border-removed/50 text-removed'
                            : active
                              ? 'border-primary text-accent-text'
                              : 'border-primary/35 text-accent-text'
                        }`}
                      >
                        {active ? <Loader2 size={13} className="animate-spin" /> : <Icon size={13} />}
                      </span>
                      <span className="min-w-0 pt-0.5">
                        <span className="block text-xs font-semibold text-foreground">
                          {item.message}
                        </span>
                        <span className="mt-0.5 block text-2xs leading-relaxed text-muted-foreground">
                          {item.detail}
                        </span>
                      </span>
                    </div>
                  )
                })}
              </div>
            </div>
          </div>
        ) : (
          <div className="grid gap-5 px-5 py-5">
            <section>
              <div className="mb-2 flex items-baseline justify-between gap-3">
                <label className="text-xs font-semibold text-foreground">
                  How many commits do you want?
                </label>
                <span className="font-mono text-2xs text-muted-foreground">2-{maxCommits}</span>
              </div>
              <div className="flex items-center gap-3 rounded-lg border border-border bg-panel2 p-2.5">
                <Button
                  variant="outline"
                  size="icon-sm"
                  disabled={count <= 2}
                  onClick={() => setCount((value) => Math.max(2, value - 1))}
                  tooltip="One fewer commit"
                >
                  <Minus />
                </Button>
                <div className="min-w-0 flex-1 text-center">
                  <div className="font-wordmark text-xl text-accent-text">{count}</div>
                  <div className="text-2xs text-sub">smaller, focused commits</div>
                </div>
                <Button
                  variant="outline"
                  size="icon-sm"
                  disabled={count >= maxCommits}
                  onClick={() => setCount((value) => Math.min(maxCommits, value + 1))}
                  tooltip="One more commit"
                >
                  <Plus />
                </Button>
              </div>
            </section>

            <section>
              <label htmlFor="commit-split-instructions" className="mb-2 block text-xs font-semibold">
                Any special instructions? <span className="font-normal text-sub">Optional</span>
              </label>
              <Textarea
                id="commit-split-instructions"
                value={instructions}
                onChange={(event) => {
                  setInstructions(event.target.value)
                  setError(null)
                }}
                rows={3}
                maxLength={4000}
                placeholder="For example: Keep tests with the feature they cover. Put settings changes last."
                className="resize-none bg-panel2 text-xs"
              />
            </section>

            {hasConflicts && (
              <div className="rounded-md border border-removed/35 bg-removed/10 px-3 py-2 text-xs text-removed">
                Resolve the conflicted files first, then generate commits.
              </div>
            )}
            <div className="flex items-center gap-2 rounded-md border border-dashed border-border px-3 py-2 text-2xs text-sub">
              <GitCommitHorizontal size={14} className="text-accent-text" />
              This creates commits right away. It does not push them.
            </div>
          </div>
        )}

        <DialogFooter className="flex-row items-center justify-end gap-2 border-t border-border px-5 py-3">
          {created ? (
            <Button size="sm" onClick={() => changeOpen(false)}>
              Done
            </Button>
          ) : error ? (
            <Button
              size="sm"
              onClick={() => {
                ai.generateCommits.reset()
                setError(null)
                setActivity([])
                setStartedAt(null)
                setElapsedSeconds(0)
              }}
            >
              Review and try again
            </Button>
          ) : (
            <>
              <Button variant="secondary" size="sm" disabled={pending} onClick={() => changeOpen(false)}>
                Cancel
              </Button>
              <Button size="sm" disabled={pending || hasConflicts} onClick={generate}>
                <Sparkles />
                {pending ? 'Generating commits…' : `Generate ${count} commits`}
              </Button>
            </>
          )}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
