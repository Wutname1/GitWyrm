/**
 * Pure client-side projection of `AgentSession.executions` into the tree the
 * Graph panel renders -- tasks.md 6.1 "no separate frontend graph truth".
 *
 * Mirrors `agentdesk::graph::{schedule, project_graph}` on the backend
 * (`src-tauri/src/agentdesk/graph.rs`) closely enough that the UI can show
 * "waiting on X" without a round trip, but this is a read-only display
 * helper -- the backend's own scheduler is what actually decides what runs
 * next. Kept here (not inline in the component) so it can be unit tested
 * without a DOM (vitest runs `src/**\/*.test.ts` in a Node env, no jsdom).
 */
import type { ExecutionRecord, SessionState } from '@/lib/bindings'

const MAX_CONCURRENT_HELPERS = 3

const ACTIVE_STATES: SessionState[] = ['preparing', 'working', 'needsInput']

function isActive(state: SessionState): boolean {
  return ACTIVE_STATES.includes(state)
}

export interface GraphTreeNode {
  execution: ExecutionRecord
  isLead: boolean
  /** Other execution ids this node is waiting to finish. */
  blockedOn: string[]
  /** True when this node's dependencies are all met but no concurrency slot
   * is free yet -- distinct from `blockedOn.length > 0` (waiting on a
   * specific sibling) even though both read as "not running yet". */
  waitingForSlot: boolean
}

/** One flat, ordered list: the lead first (if present), then every helper in
 * declaration order -- the shape `.ag-graph-tree` renders, indentation
 * decided per-row by `isLead`. */
export function buildGraphTree(executions: ExecutionRecord[]): GraphTreeNode[] {
  const finished = new Set(executions.filter((e) => e.state === 'finished').map((e) => e.executionId))
  const helpers = executions.filter((e) => e.parentExecutionId !== null)
  const activeCount = helpers.filter((e) => isActive(e.state)).length
  let slots = Math.max(0, MAX_CONCURRENT_HELPERS - activeCount)

  const blockedOnByExec = new Map<string, string[]>()
  const waitingForSlotByExec = new Set<string>()
  for (const helper of helpers) {
    if (helper.state !== 'ready' && helper.state !== 'draft') continue
    const waitingOn = (helper.dependsOn ?? []).filter((dep) => !finished.has(dep))
    if (waitingOn.length === 0) {
      if (slots > 0) {
        slots -= 1
      } else {
        waitingForSlotByExec.add(helper.executionId)
      }
    } else {
      blockedOnByExec.set(helper.executionId, waitingOn)
    }
  }

  return executions.map((execution) => ({
    execution,
    isLead: execution.parentExecutionId === null,
    blockedOn: blockedOnByExec.get(execution.executionId) ?? [],
    waitingForSlot: waitingForSlotByExec.has(execution.executionId),
  }))
}

/** Plain-language status word for one node, matching the mockup's
 * `.ag-node-status` text (`working`, `waiting`, `done`, `queued`, ...). */
export function nodeStatusLabel(node: GraphTreeNode): string {
  const { execution, blockedOn, waitingForSlot } = node
  const queued = blockedOn.length > 0 || waitingForSlot
  switch (execution.state) {
    case 'draft':
      return queued ? 'queued' : 'not started'
    case 'preparing':
      return 'starting'
    case 'ready':
      return queued ? 'queued' : 'ready'
    case 'working':
      return 'working'
    case 'needsInput':
      return execution.conflict ? 'conflict' : 'waiting'
    case 'finished':
      return 'done'
    case 'failed':
      return 'failed'
    case 'stopped':
      return 'stopped'
    case 'missingSource':
      return 'source missing'
  }
}

/** `.ag-node-dot`'s tone class family: `lead | done | working | waiting`, or
 * `undefined` for the neutral/queued dot. */
export function nodeDotTone(node: GraphTreeNode): 'lead' | 'done' | 'working' | 'waiting' | undefined {
  if (node.isLead) return 'lead'
  const label = nodeStatusLabel(node)
  if (label === 'done') return 'done'
  if (label === 'working' || label === 'starting') return 'working'
  if (label === 'waiting' || label === 'conflict') return 'waiting'
  return undefined
}

export function isNodeActive(execution: ExecutionRecord): boolean {
  return isActive(execution.state)
}

/** Summary text for the panel header, mockup `.ag-panel-summary` ("2
 * working · 1 waiting"). */
export function graphSummary(executions: ExecutionRecord[]): string {
  const working = executions.filter((e) => e.state === 'working' || e.state === 'preparing').length
  const waiting = executions.filter((e) => e.state === 'needsInput').length
  const parts: string[] = []
  if (working > 0) parts.push(`${working} working`)
  if (waiting > 0) parts.push(`${waiting} waiting`)
  if (parts.length === 0) return `${executions.length} agent${executions.length === 1 ? '' : 's'}`
  return parts.join(' · ')
}
