import { useEffect, useState } from 'react'
import { ExternalLink } from 'lucide-react'
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from '@/components/ui/dialog'
import { Input } from '@/components/ui/input'
import { Textarea } from '@/components/ui/textarea'
import { Button } from '@/components/ui/button'
import { pullRequestUrlWithDraft } from '@/lib/agentDeskPullRequest'

/**
 * Read and edit the pull request before it opens on the host.
 *
 * The title and body were drafted from the commit and the spec change, and
 * then only the title flashed past in a toast: the body was thrown away and
 * the host page opened blank, so "Create pull request" was really "open the
 * compare page". This is the review step that was missing.
 *
 * Nothing here reaches the host. Opening the page hands the text over as URL
 * parameters the host's own new-PR form reads, so the person still presses
 * the host's own button. GitWyrm never pushes, never calls a host write API,
 * and never submits a pull request on someone's behalf.
 */
export function PullRequestDraftDialog({
  open,
  onOpenChange,
  compareUrl,
  initialTitle,
  initialBody,
  /** True when the branch already has a pull request open. */
  existingUrl,
  onOpen,
}: {
  open: boolean
  onOpenChange: (open: boolean) => void
  compareUrl: string
  initialTitle: string
  initialBody: string
  existingUrl: string | null
  onOpen: (url: string) => void
}) {
  const [title, setTitle] = useState(initialTitle)
  const [body, setBody] = useState(initialBody)

  // A fresh draft replaces whatever was left from the last one, so reopening
  // never shows another result's text.
  useEffect(() => {
    if (open) {
      setTitle(initialTitle)
      setBody(initialBody)
    }
  }, [open, initialTitle, initialBody])

  const updating = existingUrl != null

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-xl">
        <DialogHeader>
          <DialogTitle>{updating ? 'Update the pull request' : 'Create a pull request'}</DialogTitle>
          <DialogDescription>
            {updating
              ? 'This branch already has a pull request. Opening it takes you to the one that exists; your edits here are for the new description you paste in.'
              : 'Read this over. Opening it fills in the host’s own form, where you press the button that actually creates it.'}
          </DialogDescription>
        </DialogHeader>

        <div className="flex flex-col gap-3">
          <label className="flex flex-col gap-1">
            <span className="text-2xs font-semibold text-sub">Title</span>
            <Input value={title} onChange={(e) => setTitle(e.target.value)} className="h-8 text-xs" />
          </label>
          <label className="flex flex-col gap-1">
            <span className="text-2xs font-semibold text-sub">Description</span>
            <Textarea
              value={body}
              onChange={(e) => setBody(e.target.value)}
              rows={8}
              className="resize-none text-xs"
              placeholder="What this changes, and why."
            />
          </label>
        </div>

        <div className="flex items-center justify-end gap-2">
          <Button variant="ghost" size="sm" onClick={() => onOpenChange(false)}>
            Not yet
          </Button>
          <Button
            size="sm"
            disabled={title.trim().length === 0}
            onClick={() => {
              onOpen(
                updating && existingUrl
                  ? existingUrl
                  : pullRequestUrlWithDraft(compareUrl, title, body)
              )
              onOpenChange(false)
            }}
          >
            <ExternalLink size={13} aria-hidden />
            {updating ? 'Open the pull request' : 'Open it on the host'}
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  )
}
