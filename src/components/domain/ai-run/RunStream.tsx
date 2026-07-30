import { Check, FileDiff, ListChecks, MessageSquare, X } from 'lucide-react'
import { cn } from '@/lib/utils'
import type { GateAnswer, RunStep } from '@/lib/bindings'
import { GateCard } from './GateCard'

/**
 * The activity stream: plain-language rows, never a terminal dump.
 *
 * Scrolls its own container. A stream that hijacks page scroll makes the rest
 * of the window unusable while a run is going, which is the opposite of the
 * promise that other tabs stay usable.
 */
export function RunStream({
  steps,
  onAnswerGate,
  onViewDiff,
  gateBusy,
}: {
  steps: RunStep[]
  onAnswerGate: (answer: GateAnswer) => void
  onViewDiff?: (path: string) => void
  gateBusy?: boolean
}) {
  if (steps.length === 0) {
    return (
      <div className="px-1 py-6 text-center text-2xs text-muted-foreground">
        Getting ready...
      </div>
    )
  }

  return (
    <div className="min-h-0 flex-1 overflow-y-auto overscroll-contain">
      <ol className="space-y-1.5 p-1">
        {steps.map((step, i) => (
          <li key={i}>
            <StreamRow
              step={step}
              onAnswerGate={onAnswerGate}
              onViewDiff={onViewDiff}
              gateBusy={gateBusy}
              // Only the newest gate is answerable; earlier ones are history.
              isLatest={i === steps.length - 1}
            />
          </li>
        ))}
      </ol>
    </div>
  )
}

function StreamRow({
  step,
  onAnswerGate,
  onViewDiff,
  gateBusy,
  isLatest,
}: {
  step: RunStep
  onAnswerGate: (answer: GateAnswer) => void
  onViewDiff?: (path: string) => void
  gateBusy?: boolean
  isLatest: boolean
}) {
  switch (step.kind) {
    case 'preflight':
      return (
        <div className="rounded-md border border-border bg-panel2 p-2.5">
          <div className="mb-1.5 flex items-center gap-1.5 text-2xs font-semibold text-sub">
            <ListChecks size={12} />
            Before starting
          </div>
          <ul className="space-y-1">
            {step.items.map((item) => (
              <li key={item.label} className="flex items-start gap-1.5 text-2xs">
                {item.done ? (
                  <Check size={11} className="mt-0.5 flex-none text-green-500" />
                ) : (
                  // Not a failure -- an honest "nothing to do here". A tick
                  // would imply work that did not happen.
                  <span aria-hidden className="mt-0.5 flex-none text-muted-foreground">
                    –
                  </span>
                )}
                <span className={cn(item.done ? 'text-sub' : 'text-muted-foreground')}>
                  {item.label}
                  {item.detail && (
                    <span className="text-muted-foreground"> — {item.detail}</span>
                  )}
                </span>
              </li>
            ))}
          </ul>
        </div>
      )

    case 'plan':
      return <Row icon={null} text={step.text} />

    case 'edit':
      return (
        <div className="flex items-center gap-2 rounded-md px-2 py-1.5 hover:bg-panel2">
          <FileDiff size={12} className="flex-none text-muted-foreground" />
          <span className="min-w-0 flex-1 truncate text-2xs text-foreground">{step.path}</span>
          <span className="flex-none font-mono text-2xs">
            {step.added > 0 && <span className="text-green-500">+{step.added}</span>}
            {step.added > 0 && step.removed > 0 && ' '}
            {step.removed > 0 && <span className="text-destructive">-{step.removed}</span>}
          </span>
          {onViewDiff && (
            <button
              type="button"
              className="flex-none rounded px-1.5 py-0.5 text-2xs text-muted-foreground hover:bg-soft hover:text-foreground"
              onClick={() => onViewDiff(step.path)}
            >
              View diff
            </button>
          )}
        </div>
      )

    case 'check':
      return (
        <div className="flex items-center gap-2 rounded-md px-2 py-1.5">
          {step.passed ? (
            <Check size={12} className="flex-none text-green-500" />
          ) : (
            <X size={12} className="flex-none text-destructive" />
          )}
          <span className="text-2xs text-foreground">
            {step.name} {step.passed ? 'passed' : 'failed'}
          </span>
          {step.detail && (
            <span className="truncate text-2xs text-muted-foreground">{step.detail}</span>
          )}
        </div>
      )

    case 'gate':
      return isLatest ? (
        <GateCard request={step.request} onAnswer={onAnswerGate} busy={gateBusy} />
      ) : (
        <Row icon={null} text={gateHistoryText(step)} muted />
      )

    case 'youSaid':
      return (
        <div className="flex items-start gap-2 rounded-md bg-soft px-2 py-1.5">
          <MessageSquare size={12} className="mt-0.5 flex-none text-accent-text" />
          <span className="text-2xs text-foreground">
            <span className="font-semibold">You said:</span> {step.text}
          </span>
        </div>
      )

    case 'note':
      return <Row icon={null} text={step.text} muted />

    case 'adapted':
      return <Row icon={null} text={step.text} />

    case 'ended':
      return <Row icon={null} text={step.detail} />
  }
}

function gateHistoryText(step: Extract<RunStep, { kind: 'gate' }>): string {
  const r = step.request
  switch (r.kind) {
    case 'addDependency':
      return `Asked about adding ${r.name}`
    case 'runInstall':
      return `Asked about running ${r.command}`
    case 'networkAccess':
      return `Asked about reaching ${r.target}`
    case 'deleteFiles':
      return `Asked about deleting ${r.paths.length} file(s)`
    case 'outsideRepo':
      return `Asked about touching ${r.path}`
    case 'unclassified':
      return `Asked about ${r.summary}`
  }
}

function Row({
  icon,
  text,
  muted,
}: {
  icon: React.ReactNode
  text: string
  muted?: boolean
}) {
  return (
    <div className="flex items-start gap-2 px-2 py-1.5">
      {icon}
      <span
        className={cn(
          'text-2xs leading-relaxed',
          muted ? 'text-muted-foreground' : 'text-foreground'
        )}
      >
        {text}
      </span>
    </div>
  )
}
