import { AlertTriangle } from 'lucide-react'
import { Button } from '@/components/ui/button'
import type { GateAnswer, GateRequest } from '@/lib/bindings'

/**
 * The card shown when a run pauses for something with a consequence.
 *
 * Three choices, always the same three. There is deliberately no "don't ask
 * again" and nothing to type: a remembered approval is a decision made once
 * and then applied to situations the user never saw, and type-to-confirm
 * teaches people to copy words rather than read them.
 */
export function GateCard({
  request,
  onAnswer,
  busy,
}: {
  request: GateRequest
  onAnswer: (answer: GateAnswer) => void
  busy?: boolean
}) {
  return (
    <div
      role="group"
      aria-label="This run needs your answer"
      className="rounded-lg border border-amber-500/40 bg-amber-500/5 p-3"
    >
      <div className="flex items-start gap-2.5">
        <AlertTriangle size={15} className="mt-0.5 flex-none text-amber-500" />
        <div className="min-w-0 flex-1">
          <div className="text-xs font-semibold text-foreground">{gateTitle(request)}</div>
          <p className="mt-1 text-2xs leading-relaxed text-sub">{gateBody(request)}</p>
          <div className="mt-2.5 flex flex-wrap gap-1.5">
            <Button
              size="sm"
              className="h-7 text-xs"
              disabled={busy}
              onClick={() => onAnswer('allowOnce')}
            >
              Allow this once
            </Button>
            <Button
              variant="secondary"
              size="sm"
              className="h-7 text-xs"
              disabled={busy}
              onClick={() => onAnswer('findAnotherWay')}
            >
              No - find another way
            </Button>
            <Button
              variant="ghost"
              size="sm"
              className="h-7 text-xs text-muted-foreground hover:text-destructive"
              disabled={busy}
              onClick={() => onAnswer('stopRun')}
            >
              Stop the run
            </Button>
          </div>
        </div>
      </div>
    </div>
  )
}

/** Consequence first, mechanism second. */
export function gateTitle(request: GateRequest): string {
  switch (request.kind) {
    case 'addDependency':
      return `Add the ${request.name} library?`
    case 'runInstall':
      return 'Install packages?'
    case 'networkAccess':
      return `Let this reach ${request.target}?`
    case 'deleteFiles':
      return request.paths.length === 1
        ? `Delete ${request.paths[0]}?`
        : `Delete ${request.paths.length} files?`
    case 'outsideRepo':
      return `Touch ${request.path}, outside this folder?`
    case 'unclassified':
      return `Allow this: ${request.summary}?`
  }
}

/** What actually happens if this is allowed. */
export function gateBody(request: GateRequest): string {
  switch (request.kind) {
    case 'addDependency':
      return `This downloads ${request.name} from the internet and adds it to your project's list of libraries. You'll see the change before anything is committed.`
    case 'runInstall':
      return `This runs ${request.command}, which downloads code from the internet onto this machine.`
    case 'networkAccess':
      return `This sends a request to ${request.target} and waits for a reply.`
    case 'deleteFiles':
      return `This removes ${request.paths.join(', ')} from your folder. You can undo the whole run afterwards.`
    case 'outsideRepo':
      // The one gate whose consequence outlives the run, so it says so.
      return `${request.path} is outside the folder you opened, so it isn't covered by undo. Changes there stay even if you undo this run.`
    case 'unclassified':
      // Shown when GitWyrm cannot tell what the AI is asking for. Says exactly
      // that rather than dressing it up as a known kind of request, which
      // would tell the user the wrong thing about what they are approving.
      return `The AI asked to do something GitWyrm doesn't recognise: ${request.summary}. Only allow it if you understand what it will do.`
  }
}
