import { useEffect, useMemo, useRef, useState } from 'react'
import { Tag } from 'lucide-react'
import { Input } from '@/components/ui/input'
import { FormDialog } from '@/components/ui/form-dialog'
import { InProgressModal } from '@/components/modals/InProgressModal'
import { classifyError } from '@/lib/errorClass'
import { useTags } from '@/hooks/useGitQueries'
import { useGitMutations } from '@/hooks/useGitMutations'
import { useTagSync } from '@/hooks/useTagSync'
import { formatRelativeTime, shortSha } from '@/lib/gitDisplay'
import { refNameError } from '@/lib/refName'
import { bumpSemver, formatSemver, highestSemver } from '@/lib/semver'
import { useUiStore } from '@/stores/uiStore'
import { useActiveRepo, useWorkspaceStore } from '@/stores/workspaceStore'

export function NewTagModal() {
  const open = useUiStore((s) => s.activeModal === 'newTag')
  const closeModal = useUiStore((s) => s.closeModal)
  const targetSha = useUiStore((s) => s.tagTargetSha)

  const repo = useActiveRepo()
  const tags = useTags(repo?.id ?? null)
  const m = useGitMutations(repo?.id ?? null)

  // The dialog is the only place that needs the host's name, and only while it
  // is open, so the remote lookup is gated on that.
  const { hostLabel, hasRemote } = useTagSync(repo?.id ?? null, open)

  // The checkbox both reads and writes the saved preference, so whichever way
  // the user left it last is how it comes back. Subscribe to the app default
  // and this repo's override so the value stays reactive if either changes in
  // Settings > Tags while the dialog is closed.
  const appPushOnCreate = useWorkspaceStore((s) => s.tagPushOnCreate)
  const repoPushOnCreate = useWorkspaceStore((s) =>
    repo ? s.tagOverridesByRepo[repo.path]?.pushOnCreate : undefined
  )
  const setTagPushOnCreate = useWorkspaceStore((s) => s.setTagPushOnCreate)
  const setRepoTagOverride = useWorkspaceStore((s) => s.setRepoTagOverride)
  const tagPushOnCreate = repoPushOnCreate ?? appPushOnCreate

  const [name, setName] = useState('')
  const [message, setMessage] = useState('')
  const [push, setPush] = useState(false)
  /**
   * The run that replaced this dialog. Set on submit and cleared when the
   * progress modal is dismissed; `error` is null until the mutation fails.
   */
  const [run, setRun] = useState<{
    name: string
    push: boolean
    /** Raw failure text, shown in the copyable block. Null while still working. */
    error: string | null
    /** Plain-language version of the same failure, shown as the subtext. */
    friendly?: string
  } | null>(null)

  // Remember the choice for next time. A repo that has its own rule keeps it --
  // overwriting the app default from here would silently change every other
  // repository that follows it.
  const rememberPush = (next: boolean) => {
    setPush(next)
    if (repoPushOnCreate !== undefined && repo) {
      setRepoTagOverride(repo.path, { pushOnCreate: next })
    } else {
      setTagPushOnCreate(next)
    }
  }

  // Blocks a second submit within the same tick -- `isPending` does not flip
  // synchronously after `mutate`, so an Enter keypress and a fast button click
  // could otherwise both fire the create before the first one registers.
  const submitting = useRef(false)

  // Read through a ref so toggling the checkbox -- which saves the preference --
  // does not re-run this effect and wipe the name the user is typing.
  const rememberedPush = useRef(tagPushOnCreate)
  rememberedPush.current = tagPushOnCreate

  useEffect(() => {
    if (open) {
      setName('')
      setMessage('')
      submitting.current = false
      // Start from the remembered choice each time the dialog opens.
      setPush(rememberedPush.current)
    }
  }, [open])

  const existing = useMemo(
    () => new Set((tags.data ?? []).map((t) => t.name)),
    [tags.data]
  )

  // When the repo tags with semver, offer one-tap buttons for the next major,
  // minor, and patch versions off the highest existing tag.
  const quickPicks = useMemo(() => {
    const latest = highestSemver([...existing])
    if (!latest) return []
    return (['major', 'minor', 'patch'] as const).map((bump) => ({
      bump,
      value: formatSemver(bumpSemver(latest, bump)),
    }))
  }, [existing])

  // The most recently created tag, by date rather than by name -- a repo can tag
  // an old commit or use names that don't sort chronologically.
  const lastTag = useMemo(() => {
    const all = tags.data ?? []
    if (all.length === 0) return null
    return all.reduce((latest, t) => (t.time > latest.time ? t : latest))
  }, [tags.data])

  const trimmed = name.trim()
  const error = refNameError(trimmed, [...existing], 'tag')

  const canCreate = trimmed !== '' && !error && !m.createTag.isPending
  const sendIt = push && hasRemote

  const create = () => {
    if (!canCreate || submitting.current) return
    submitting.current = true
    // Hand off to the progress modal and close this one immediately. Beyond
    // showing the wait, this is what stops the "there's already a tag called X"
    // flash: creating the tag refetches the tag list, and this dialog would
    // otherwise still be mounted with the name typed in, seeing its own new tag
    // as a collision for the frame before it closed.
    setRun({ name: trimmed, push: sendIt, error: null })
    closeModal()
    m.createTag.mutate(
      { name: trimmed, sha: targetSha ?? '', message: message.trim(), push: sendIt, quiet: true },
      {
        onSuccess: () => setRun(null),
        onError: (reason) => {
          submitting.current = false
          const { message: friendly, raw } = classifyError(reason)
          setRun((current) => (current ? { ...current, error: raw, friendly } : current))
        },
      }
    )
  }

  // The two are mutually exclusive: submitting closes this dialog and opens the
  // progress modal, so only one is ever mounted.
  if (run != null) {
    return (
      <InProgressModal
        open
        title={run.push ? `Creating and sending ${run.name}` : `Creating tag ${run.name}`}
        subtext={
          run.error != null
            ? run.friendly
            : run.push
              ? `Sending it to ${hostLabel}.`
              : 'This only takes a moment.'
        }
        error={run.error}
        errorTitle="Could not create the tag"
        onClose={() => {
          m.createTag.reset()
          setRun(null)
        }}
      />
    )
  }

  return (
    <FormDialog
      open={open}
      onOpenChange={(o) => !o && closeModal()}
      icon={<Tag size={15} strokeWidth={1.9} />}
      title="New tag"
      submitLabel={sendIt ? 'Create and send' : 'Create tag'}
      pendingLabel={sendIt ? 'Sending…' : 'Creating…'}
      canSubmit={canCreate}
      pending={m.createTag.isPending}
      onSubmit={create}
    >
      <div className="grid gap-1.5">
        <label className="text-2xs font-semibold text-sub">Tag name</label>
        <Input
          value={name}
          onChange={(e) => setName(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === 'Enter') create()
          }}
          placeholder="v1.0.0"
          className="h-auto bg-background py-1.5 font-mono text-xs"
          autoFocus
        />

        <div className="flex flex-wrap items-center gap-x-3 gap-y-1.5">
          {quickPicks.length > 0 && (
            <div className="flex flex-wrap items-center gap-1.5">
              <span className="text-2xs text-muted-foreground">Recommended:</span>
              {quickPicks.map((pick) => {
                const selected = trimmed === pick.value
                return (
                  <button
                    key={pick.bump}
                    type="button"
                    aria-pressed={selected}
                    onClick={() => setName(pick.value)}
                    className={
                      selected
                        ? 'flex items-baseline gap-1.5 rounded border border-primary bg-primary/15 px-2 py-1 text-2xs text-accent-text'
                        : 'flex items-baseline gap-1.5 rounded border border-border bg-panel px-2 py-1 text-2xs text-sub hover:border-primary hover:text-accent-text'
                    }
                  >
                    <span className="font-mono">{pick.value}</span>
                    <span className="capitalize text-muted-foreground">{pick.bump}</span>
                  </button>
                )
              })}
            </div>
          )}

          {lastTag && (
            <span className="text-2xs text-muted-foreground">
              Last tag: <span className="font-mono text-sub">{lastTag.name}</span>
              {', '}
              {formatRelativeTime(lastTag.time)}
            </span>
          )}
        </div>

        <p className="min-h-[15px] text-2xs leading-tight text-removed">{error ?? ''}</p>
      </div>

      <div className="grid gap-1.5">
        <label className="text-2xs font-semibold text-sub">
          Message <span className="font-normal text-muted-foreground">(optional)</span>
        </label>
        <Input
          value={message}
          onChange={(e) => setMessage(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === 'Enter') create()
          }}
          placeholder="What this release is"
          className="h-auto bg-background py-1.5 text-xs"
        />
        <p className="text-2xs leading-tight text-muted-foreground">
          Add a message to make it an annotated tag; leave blank for a simple one.
        </p>
      </div>

      {hasRemote ? (
        <label className="flex cursor-pointer items-center gap-2 text-xs text-sub">
          <input
            type="checkbox"
            checked={push}
            onChange={(e) => rememberPush(e.target.checked)}
            className="size-3.5 accent-[var(--gw-accent)]"
          />
          Also send it to {hostLabel} now
        </label>
      ) : (
        <p className="text-2xs text-muted-foreground">
          This tag stays on your computer. Add a remote to be able to send it.
        </p>
      )}

      <p className="text-2xs text-muted-foreground">
        Tags{' '}
        {targetSha ? (
          <>
            the commit <span className="font-mono text-sub">{shortSha(targetSha)}</span>.
          </>
        ) : (
          'the latest commit on this branch.'
        )}
      </p>
    </FormDialog>
  )
}
