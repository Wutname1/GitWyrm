import { useEffect, useRef, useState } from 'react'
import { Check, GitCommitHorizontal, Sparkles } from 'lucide-react'
import { toast } from 'sonner'
import logoUrl from '@/assets/logo.png'
import { cn } from '@/lib/utils'
import { Input } from '@/components/ui/input'
import { Textarea } from '@/components/ui/textarea'
import { useAiConfigured, useAiMutations } from '@/hooks/useAi'
import { useBranches, useStatus } from '@/hooks/useGitQueries'
import { useGitMutations } from '@/hooks/useGitMutations'
import { useActiveRepo, useWorkspaceStore } from '@/stores/workspaceStore'
import { useUiStore } from '@/stores/uiStore'

export function CommitMessageForm() {
  const repo = useActiveRepo()
  const status = useStatus(repo?.id ?? null)
  const branches = useBranches(repo?.id ?? null)
  const m = useGitMutations(repo?.id ?? null)
  const [msg, setMsg] = useState('')
  const [desc, setDesc] = useState('')
  const [justGenerated, setJustGenerated] = useState(false)
  const generatedTimer = useRef<number | null>(null)

  const ai = useAiMutations()
  const configured = useAiConfigured()
  const aiProvider = useWorkspaceStore((s) => s.aiProvider)
  const aiModel = useWorkspaceStore((s) => s.aiModel)
  const showSettings = useUiStore((s) => s.showSettings)
  const aiReady =
    aiProvider != null &&
    aiModel != null &&
    (configured.data ?? []).some((c) => c.id === aiProvider)

  const stagedCount = status.data?.staged.length ?? 0
  const currentBranch = branches.data?.local.find((b) => b.is_head)?.name ?? 'HEAD'
  const canCommit =
    stagedCount > 0 &&
    msg.trim().length > 0 &&
    !m.createCommit.isPending &&
    !ai.generate.isPending

  useEffect(
    () => () => {
      if (generatedTimer.current != null) window.clearTimeout(generatedTimer.current)
    },
    []
  )

  const doCommit = () => {
    if (!canCommit) {
      toast(stagedCount ? 'Enter a commit message' : 'Stage files to commit')
      return
    }
    m.createCommit.mutate(
      { summary: msg, description: desc },
      {
        onSuccess: () => {
          setMsg('')
          setDesc('')
        },
      }
    )
  }

  const doGenerate = () => {
    if (!aiReady) {
      showSettings('ai')
      return
    }
    if (ai.generate.isPending) return
    if (!repo || stagedCount === 0) {
      toast('Stage files to generate a message')
      return
    }
    setJustGenerated(false)
    ai.generate.mutate(
      { repoId: repo.id, provider: aiProvider!, model: aiModel! },
      {
        onSuccess: (r) => {
          setMsg(r.summary)
          setDesc(r.description)
          setJustGenerated(true)
          if (generatedTimer.current != null) window.clearTimeout(generatedTimer.current)
          generatedTimer.current = window.setTimeout(() => setJustGenerated(false), 1400)
        },
        onError: (e) => toast.error(String(e)),
      }
    )
  }

  const generating = ai.generate.isPending

  return (
    <div className="min-h-[183px] flex-none border-t border-border bg-panel2 px-3 pb-[13px] pt-[11px]">
      <div className="relative mb-[9px] min-h-[116px]">
        <div className="relative mb-[7px] rounded-md">
          <Input
            value={generating ? '' : msg}
            onChange={(e) => setMsg(e.target.value)}
            disabled={generating}
            placeholder={generating ? '' : 'Summary (required)'}
            className={cn(
              'h-auto bg-background py-2 pl-2.5 pr-9 text-xs transition-[opacity,filter] duration-200',
              generating && 'opacity-[0.18] saturate-[0.35]'
            )}
          />
          <button
            onClick={doGenerate}
            disabled={generating}
            aria-label={generating ? 'Generating commit message' : 'Generate commit message with AI'}
            title={
              generating
                ? 'Generating commit message'
                : !aiReady
                  ? 'Set up an AI provider to generate messages'
                  : stagedCount === 0
                    ? 'Stage files to generate a message'
                    : 'Generate commit message with AI'
            }
            className={cn(
              'absolute right-1.5 top-1/2 z-20 flex size-6 -translate-y-1/2 items-center justify-center overflow-hidden rounded-[5px] border border-transparent text-sub',
              generating
                ? 'wyrm-ai-trigger-active cursor-wait'
                : justGenerated
                  ? 'border-primary/25 bg-soft text-primary'
                  : 'cursor-pointer hover:bg-panel3 hover:text-foreground'
            )}
          >
            {justGenerated && !generating ? (
              <Check size={13} strokeWidth={2.3} />
            ) : (
              <Sparkles size={13} className={cn(generating && 'wyrm-ai-spark')} />
            )}
          </button>
        </div>
        <Textarea
          value={generating ? '' : desc}
          onChange={(e) => setDesc(e.target.value)}
          disabled={generating}
          placeholder={generating ? '' : 'Extended description…'}
          rows={2}
          className={cn(
            'wyrm-commit-description w-full bg-background px-2.5 py-2 text-[11.5px] transition-[opacity,filter] duration-200',
            generating && 'opacity-[0.18] saturate-[0.35]'
          )}
        />
        {generating && (
          <div className="wyrm-ai-stage" role="status" aria-live="polite">
            <div className="wyrm-ai-logo-wrap" aria-hidden="true">
              <div className="wyrm-ai-energy-ring" />
              <img src={logoUrl} alt="" className="wyrm-ai-logo" />
              <i className="wyrm-ai-eye wyrm-ai-eye-left" />
              <i className="wyrm-ai-eye wyrm-ai-eye-right" />
              <svg className="wyrm-ai-graph" viewBox="0 0 40 27">
                <path d="M5 5h18v17h12" />
                <circle cx="5" cy="5" r="3" />
                <circle cx="23" cy="5" r="3" />
                <circle cx="35" cy="22" r="3" />
              </svg>
            </div>
            <div className="wyrm-ai-stage-copy">
              <div className="wyrm-ai-stage-label">Generating commit message</div>
              <div className="wyrm-ai-stage-status">
                Reading {stagedCount} staged file{stagedCount === 1 ? '' : 's'}…
              </div>
              <div className="wyrm-ai-stage-detail">Writing a summary and description</div>
            </div>
          </div>
        )}
      </div>
      <button
        onClick={doCommit}
        className={cn(
          'flex h-[34px] w-full items-center justify-center gap-2 rounded-md border text-[12.5px] font-semibold transition-colors',
          canCommit
            ? 'cursor-pointer border-primary/50 bg-soft text-primary hover:border-primary hover:bg-primary hover:text-primary-foreground'
            : 'cursor-not-allowed border-transparent bg-panel3 text-muted-foreground'
        )}
      >
        <GitCommitHorizontal size={15} strokeWidth={2} />
        Commit {stagedCount} file{stagedCount === 1 ? '' : 's'} to {currentBranch}
      </button>
    </div>
  )
}
