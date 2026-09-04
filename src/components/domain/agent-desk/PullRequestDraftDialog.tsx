import { useEffect, useState } from 'react'
import { ClipboardCopy, ExternalLink } from 'lucide-react'
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from '@/components/ui/dialog'
import { Input } from '@/components/ui/input'
import { Textarea } from '@/components/ui/textarea'
import { Button } from '@/components/ui/button'
import { pullRequestUrlWithDraft } from '@/lib/agentDeskPullRequest'
import { copyToClipboard } from '@/lib/clipboard'

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
              ? 'This branch already has a pull request. Edit the description here, copy it, then open the pull request and paste it in.'
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
          {/*
            The update path opens the existing pull request's own page, which
            has no form to prefill: `pullRequestUrlWithDraft` works by putting
            title and body in the query string of a host's COMPARE page, and
            an open pull request's page ignores those parameters.

            So the edits made above genuinely cannot travel in the link. They
            used to be discarded silently anyway, under a description telling
            the person their edits were "for the new description you paste in"
            -- with nothing anywhere to paste from. Copying them is what makes
            that sentence true.
          */}
          {updating && (
            <Button
              variant="ghost"
              size="sm"
              onClick={() => void copyToClipboard(body, 'Description copied. Paste it into the pull request.')}
            >
              <ClipboardCopy size={13} aria-hidden />
              Copy the description
            </Button>
          )}
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
