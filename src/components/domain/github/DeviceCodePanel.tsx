import { Check, Copy, ExternalLink } from 'lucide-react'
import { useState } from 'react'
import { Button } from '@/components/ui/button'
import { copyToClipboard } from '@/lib/clipboard'

interface DeviceCodePanelProps {
  userCode: string
  verificationUri: string
  onCancel: () => void
}

/**
 * The waiting half of a device-code sign-in: show the code, let the user copy
 * it, then send them to GitHub. The browser is opened by the button here rather
 * than automatically on start, so the code is on screen and in hand before the
 * user lands on the page asking for it.
 */
export function DeviceCodePanel({ userCode, verificationUri, onCancel }: DeviceCodePanelProps) {
  const [copied, setCopied] = useState(false)
  const [opened, setOpened] = useState(false)

  const copy = async () => {
    if (await copyToClipboard(userCode, 'Code copied')) {
      setCopied(true)
      setTimeout(() => setCopied(false), 2000)
    }
  }

  const openGithub = async () => {
    const { openUrl } = await import('@tauri-apps/plugin-opener')
    await openUrl(verificationUri)
    setOpened(true)
  }

  return (
    <div className="grid gap-3 rounded-md border border-border bg-background p-3">
      <div className="grid gap-1.5">
        <span className="text-xs text-sub">Step 1: copy this code</span>
        <div className="flex items-center gap-2">
          <span className="select-all rounded border border-border px-2.5 py-1.5 font-mono text-base font-bold tracking-widest text-foreground">
            {userCode}
          </span>
          <Button variant="secondary" size="sm" className="h-8 text-xs" onClick={copy}>
            {copied ? <Check size={12} /> : <Copy size={12} />}
            {copied ? 'Copied' : 'Copy'}
          </Button>
        </div>
      </div>

      <div className="grid gap-1.5">
        <span className="text-xs text-sub">Step 2: enter it on GitHub</span>
        <Button size="sm" className="h-8 justify-self-start text-xs" onClick={openGithub}>
          <ExternalLink size={12} />
          {opened ? 'Open GitHub again' : 'Open GitHub'}
        </Button>
      </div>

      <div className="flex items-center gap-2 border-t border-border pt-2">
        <span className="text-2xs text-muted-foreground">
          {opened ? 'Waiting for you to finish on GitHub…' : 'Waiting for you to enter the code…'}
        </span>
        <Button variant="ghost" size="sm" className="h-6 px-2 text-2xs" onClick={onCancel}>
          Cancel
        </Button>
      </div>
    </div>
  )
}
