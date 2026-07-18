import { useEffect, useRef, useState } from 'react'
import { Check, ChevronDown, GitCommitHorizontal, Sparkles, Upload } from 'lucide-react'
import { toast } from 'sonner'
import logoUrl from '@/assets/logo.png'
import { cn } from '@/lib/utils'
import { Input } from '@/components/ui/input'
import { Textarea } from '@/components/ui/textarea'
import {
  Tooltip,
  TooltipButton,
  TooltipContent,
  TooltipTrigger,
} from '@/components/ui/tooltip'
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuLabel,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu'
import { useAiConfigured, useAiMutations } from '@/hooks/useAi'
import { useBranches, useStatus } from '@/hooks/useGitQueries'
import { useGitMutations } from '@/hooks/useGitMutations'
import {
  useActiveRepo,
  useWorkspaceStore,
  type CommitButtonMode,
} from '@/stores/workspaceStore'
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
  const commitButtonMode = useWorkspaceStore((s) => s.commitButtonMode)
  const setCommitButtonMode = useWorkspaceStore((s) => s.setCommitButtonMode)
  const aiReady =
    aiProvider != null &&
    aiModel != null &&
    (configured.data ?? []).some((c) => c.id === aiProvider)

  const stagedCount = status.data?.staged.length ?? 0
  const currentBranch = branches.data?.local.find((b) => b.is_head)?.name ?? 'HEAD'
  const pushPending = m.push.isPending
  const canCommit =
    stagedCount > 0 &&
    msg.trim().length > 0 &&
    !m.createCommit.isPending &&
    !pushPending &&
    !ai.generate.isPending

  useEffect(
    () => () => {
      if (generatedTimer.current != null) window.clearTimeout(generatedTimer.current)
    },
    []
  )

  const doCommit = (mode: CommitButtonMode = commitButtonMode) => {
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
          if (mode === 'commit_push') m.push.mutate()
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
          <TooltipButton
            onClick={doGenerate}
            disabled={generating}
            aria-label={generating ? 'Generating commit message' : 'Generate commit message with AI'}
            tooltip={
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
          </TooltipButton>
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
      <div
        className={cn(
          'flex h-[34px] w-full overflow-hidden rounded-md border transition-colors',
          canCommit
            ? 'border-primary/50 bg-soft text-primary hover:border-primary'
            : 'cursor-not-allowed border-transparent bg-panel3 text-muted-foreground'
        )}
      >
        <button
          onClick={() => doCommit()}
          disabled={!canCommit}
          className={cn(
            'flex flex-1 items-center justify-center gap-2 text-[12.5px] font-semibold transition-colors',
            canCommit
              ? 'cursor-pointer hover:bg-primary hover:text-primary-foreground'
              : 'cursor-not-allowed'
          )}
        >
          {commitButtonMode === 'commit_push' ? (
            <Upload size={14} strokeWidth={2} />
          ) : (
            <GitCommitHorizontal size={15} strokeWidth={2} />
          )}
          {pushPending
            ? 'Pushing…'
            : commitButtonMode === 'commit_push'
              ? `Commit & push to ${currentBranch}`
              : `Commit ${stagedCount} file${stagedCount === 1 ? '' : 's'} to ${currentBranch}`}
        </button>
        <Tooltip>
          <DropdownMenu>
            <TooltipTrigger asChild>
              <DropdownMenuTrigger asChild>
                <button
                  aria-label="Change default commit action"
                  className={cn(
                    'flex w-[30px] items-center justify-center border-l transition-colors',
                    canCommit
                      ? 'cursor-pointer border-primary/40 hover:bg-primary hover:text-primary-foreground'
                      : 'cursor-pointer border-border/60 text-sub hover:bg-panel2 hover:text-foreground'
                  )}
                >
                  <ChevronDown size={14} strokeWidth={2.2} />
                </button>
              </DropdownMenuTrigger>
            </TooltipTrigger>
            <TooltipContent>Change default commit action</TooltipContent>
            <DropdownMenuContent align="end" className="w-56">
              <DropdownMenuLabel className="text-xs text-sub">
                Default commit button action
              </DropdownMenuLabel>
              <DropdownMenuSeparator />
              <DropdownMenuRadioGroup
                value={commitButtonMode}
                onValueChange={(v) => setCommitButtonMode(v as CommitButtonMode)}
              >
                <DropdownMenuRadioItem value="commit" className="text-xs">
                  <GitCommitHorizontal size={13} className="text-current" />
                  Commit only
                </DropdownMenuRadioItem>
                <DropdownMenuRadioItem value="commit_push" className="text-xs">
                  <Upload size={13} className="text-current" />
                  Commit & push
                </DropdownMenuRadioItem>
              </DropdownMenuRadioGroup>
            </DropdownMenuContent>
          </DropdownMenu>
        </Tooltip>
      </div>
    </div>
  )
}
