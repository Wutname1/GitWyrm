import {
  HoverCard,
  HoverCardTrigger,
  HoverCardContent,
} from 'gitwyrm-mockup'
import { GitCommit, Mail } from 'lucide-react'

export function CommitAuthor() {
  return (
    <div style={{ padding: 20 }}>
      <HoverCard defaultOpen open>
        <HoverCardTrigger asChild>
          <span className="cursor-pointer text-sm font-medium text-foreground underline decoration-dotted">
            Jeremy Nichols
          </span>
        </HoverCardTrigger>
        <HoverCardContent
          align="start"
          onEscapeKeyDown={(e) => e.preventDefault()}
          onPointerDownOutside={(e) => e.preventDefault()}
        >
          <div className="flex flex-col gap-2">
            <div className="flex items-center gap-3">
              <div className="flex size-9 items-center justify-center rounded-full bg-accent text-sm font-semibold text-accent-foreground">
                JN
              </div>
              <div>
                <div className="text-sm font-medium text-foreground">Jeremy Nichols</div>
                <div className="text-xs text-muted-foreground">jeremy12@gmail.com</div>
              </div>
            </div>
            <div className="flex items-center gap-2 text-xs text-muted-foreground">
              <GitCommit style={{ width: 13, height: 13 }} />
              1,284 commits · last on Jul 22
            </div>
          </div>
        </HoverCardContent>
      </HoverCard>
    </div>
  )
}

export function CommitPreview() {
  return (
    <div style={{ padding: 20 }}>
      <HoverCard defaultOpen open>
        <HoverCardTrigger asChild>
          <code className="cursor-pointer rounded bg-panel3 px-1.5 py-0.5 font-mono text-xs text-foreground">
            f6cf72d
          </code>
        </HoverCardTrigger>
        <HoverCardContent
          align="start"
          onEscapeKeyDown={(e) => e.preventDefault()}
          onPointerDownOutside={(e) => e.preventDefault()}
        >
          <div className="flex flex-col gap-2">
            <div className="text-sm font-medium text-foreground">
              One-tap version buttons when tagging a release
            </div>
            <div className="flex items-center gap-2 text-xs text-muted-foreground">
              <Mail style={{ width: 13, height: 13 }} />
              Jeremy Nichols committed 3 days ago
            </div>
            <div className="text-xs text-muted-foreground">
              2 files changed · +48 −12
            </div>
          </div>
        </HoverCardContent>
      </HoverCard>
    </div>
  )
}
