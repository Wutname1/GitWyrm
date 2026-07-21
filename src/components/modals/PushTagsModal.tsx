import { useEffect, useState } from 'react'
import { Tag } from 'lucide-react'
import { DialogDescription } from '@/components/ui/dialog'
import { FormDialog } from '@/components/ui/form-dialog'
import { useGitMutations } from '@/hooks/useGitMutations'
import { useTagSync } from '@/hooks/useTagSync'
import { plural } from '@/lib/gitDisplay'
import { useUiStore } from '@/stores/uiStore'
import { useActiveRepo, useWorkspaceStore } from '@/stores/workspaceStore'

/**
 * Offered after a push that left local-only tags behind. Both answers can be
 * remembered, so a user who always wants tags sent -- or never does -- stops
 * being asked after the first time.
 */
export function PushTagsModal() {
  const names = useUiStore((s) => s.tagsToPush)
  const promptPushTags = useUiStore((s) => s.promptPushTags)

  const repo = useActiveRepo()
  const m = useGitMutations(repo?.id ?? null)
  const open = names != null && names.length > 0
  const { hostLabel } = useTagSync(repo?.id ?? null, open)

  const setTagPushDefault = useWorkspaceStore((s) => s.setTagPushDefault)

  const [remember, setRemember] = useState(false)

  useEffect(() => {
    if (open) setRemember(false)
  }, [open])

  const close = () => promptPushTags([])

  const send = async () => {
    if (!names) return
    // Remembering "always" here is what makes the next push silent.
    if (remember) setTagPushDefault('always')
    for (const tag of names) {
      await m.pushTag.mutateAsync({ name: tag.name })
    }
    close()
  }

  const skip = () => {
    if (remember) setTagPushDefault('never')
    close()
  }

  const count = names?.length ?? 0

  return (
    <FormDialog
      open={open}
      onOpenChange={(next) => {
        // Dismissing without choosing is a "not now" that is never remembered.
        if (!next) close()
      }}
      icon={<Tag size={15} strokeWidth={1.9} />}
      title={`Send ${plural(count, 'tag')} too?`}
      cancelLabel="Not now"
      submitLabel={`Send to ${hostLabel}`}
      pendingLabel="Sending…"
      canSubmit={!m.pushTag.isPending}
      pending={m.pushTag.isPending}
      onSubmit={() => void send()}
      footerExtra={
        <label className="flex cursor-pointer items-center gap-2 text-2xs text-sub">
          <input
            type="checkbox"
            checked={remember}
            onChange={(e) => setRemember(e.target.checked)}
            className="size-3.5 accent-[var(--gw-accent)]"
          />
          Always do this
        </label>
      }
    >
      <DialogDescription className="text-xs leading-relaxed text-sub">
        Your changes went to {hostLabel}, but {count === 1 ? 'this tag is' : 'these tags are'} still
        only on your computer. Tags are not sent with a normal push.
      </DialogDescription>

      <ul className="grid max-h-40 gap-1 overflow-y-auto">
        {(names ?? []).map((tag) => (
          <li key={tag.name} className="flex items-baseline gap-2">
            <span className="font-mono text-xs text-foreground">{tag.name}</span>
            {tag.carriesCommits && (
              <span className="text-2xs text-muted-foreground">
                also sends the commits it marks
              </span>
            )}
          </li>
        ))}
      </ul>
    </FormDialog>
  )
}
