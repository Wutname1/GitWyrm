import { useState } from 'react'
import { GitCommitHorizontal, Loader2, Sparkles } from 'lucide-react'
import { toast } from 'sonner'
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
  const canCommit = stagedCount > 0 && msg.trim().length > 0 && !m.createCommit.isPending

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
    ai.generate.mutate(
      { repoId: repo.id, provider: aiProvider!, model: aiModel! },
      {
        onSuccess: (r) => {
          setMsg(r.summary)
          setDesc(r.description)
        },
        onError: (e) => toast.error(String(e)),
      }
    )
  }

  const generating = ai.generate.isPending

  return (
    <div className="flex-none border-t border-border bg-panel2 px-3.5 pb-[13px] pt-[11px]">
      <div className={cn('relative mb-[7px] rounded-md', generating && 'wyrm-ai-shimmer')}>
        <Input
          value={generating ? '' : msg}
          onChange={(e) => setMsg(e.target.value)}
          disabled={generating}
          placeholder={generating ? 'Generating…' : 'Summary (required)'}
          className="h-auto bg-background py-2 pl-2.5 pr-9 text-xs"
        />
        <button
          onClick={doGenerate}
          disabled={ai.generate.isPending}
          title={
            !aiReady
              ? 'Set up an AI provider to generate messages'
              : stagedCount === 0
                ? 'Stage files to generate a message'
                : 'Generate commit message with AI'
          }
          className={cn(
            'absolute right-1.5 top-1/2 flex size-6 -translate-y-1/2 items-center justify-center rounded-[5px] text-sub',
            ai.generate.isPending
              ? 'cursor-not-allowed opacity-50'
              : 'cursor-pointer hover:bg-panel3 hover:text-foreground'
          )}
        >
          {generating ? (
            <Loader2 size={13} className="animate-spin" />
          ) : (
            <Sparkles size={13} />
          )}
        </button>
      </div>
      <div className={cn('mb-[9px] rounded-md', generating && 'wyrm-ai-shimmer')}>
        <Textarea
          value={generating ? '' : desc}
          onChange={(e) => setDesc(e.target.value)}
          disabled={generating}
          placeholder={generating ? 'Writing description…' : 'Extended description…'}
          rows={2}
          className="min-h-0 w-full resize-none bg-background px-2.5 py-2 text-[11.5px]"
        />
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
