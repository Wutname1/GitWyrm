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
import type { ExecutionRecord } from "@/lib/bindings";
import { runIsActive, runStoppedBadly } from "@/lib/agentDeskResult";

/**
 * How many helpers may run at once.
 *
 * Hand-copied from `MAX_CONCURRENT_HELPERS` in
 * `src-tauri/src/agentdesk/graph.rs`, which is the real scheduler and the only
 * authority on this number. Specta exports commands and types but not bare
 * constants, so there is no generated binding to import; the test in
 * `agentGraphProjection.test.ts` reads the Rust source and fails if the two
 * ever disagree, which is the guard this copy needs to be safe.
 */
export const MAX_CONCURRENT_HELPERS = 3;

/**
 * The same "is a run going" rule the composer and the transcript use.
 *
 * This was a second definition of one rule over one type. It happened to
 * agree -- but the last time this rule existed in four hand-written copies,
 * one of them omitted `needsInput` and the transcript went silent while an
 * agent waited for an answer. Agreement that is not enforced is a coincidence
 * with a shelf life.
 */
const isActive = runIsActive;

export interface GraphTreeNode {
  execution: ExecutionRecord;
  isLead: boolean;
  /** Other execution ids this node is waiting to finish. */
  blockedOn: string[];
  /** True when this node's dependencies are all met but no concurrency slot
   * is free yet -- distinct from `blockedOn.length > 0` (waiting on a
   * specific sibling) even though both read as "not running yet". */
  waitingForSlot: boolean;
}

/** One flat, ordered list: the lead first (if present), then every helper in
 * declaration order -- the shape `.ag-graph-tree` renders, indentation
 * decided per-row by `isLead`. */
/**
 * Whether an execution is a helper the scheduler counts, mirroring
 * `agentdesk::graph`'s own filter: a child of the lead that is NOT the lead's
 * review turn.
 *
 * The review turn is created with the lead as its parent and seeded
 * `Preparing`, so a rule of "has a parent" swept it in as a fourth helper.
 * For the whole of every lead review that meant one phantom agent in the
 * header count and one genuinely-ready helper falsely marked queued -- on the
 * surface a person reads to decide whether a run still needs them.
 *
 * `reviewExecutionId` has been on the record and exported to the frontend
 * since the review turn shipped, and was referenced nowhere outside the
 * generated bindings.
 */
export function isSchedulableHelper(
  execution: Pick<ExecutionRecord, "executionId" | "parentExecutionId">,
  all: Pick<ExecutionRecord, "reviewExecutionId">[],
): boolean {
  if (execution.parentExecutionId === null) return false;
  return !all.some((e) => e.reviewExecutionId === execution.executionId);
}

export function buildGraphTree(executions: ExecutionRecord[]): GraphTreeNode[] {
  const finished = new Set(
    executions.filter((e) => e.state === "finished").map((e) => e.executionId),
  );
  const helpers = executions.filter((e) => isSchedulableHelper(e, executions));
  const activeCount = helpers.filter((e) => isActive(e.state)).length;
  let slots = Math.max(0, MAX_CONCURRENT_HELPERS - activeCount);

  const blockedOnByExec = new Map<string, string[]>();
  const waitingForSlotByExec = new Set<string>();
  for (const helper of helpers) {
    if (helper.state !== "ready" && helper.state !== "draft") continue;
    const waitingOn = (helper.dependsOn ?? []).filter(
      (dep) => !finished.has(dep),
    );
    if (waitingOn.length === 0) {
      if (slots > 0) {
        slots -= 1;
      } else {
        waitingForSlotByExec.add(helper.executionId);
      }
    } else {
      blockedOnByExec.set(helper.executionId, waitingOn);
    }
  }

  // The lead and its real helpers -- not the lead's own review turn, which is
  // a child of the lead and would otherwise render as a fourth agent for the
  // whole of every review.
  return executions
    .filter(
      (e) => e.parentExecutionId === null || isSchedulableHelper(e, executions),
    )
    .map((execution) => ({
      execution,
      isLead: execution.parentExecutionId === null,
      blockedOn: blockedOnByExec.get(execution.executionId) ?? [],
      waitingForSlot: waitingForSlotByExec.has(execution.executionId),
    }));
}

/** Plain-language status word for one node, matching the mockup's
 * `.ag-node-status` text (`working`, `waiting`, `done`, `queued`, ...). */
export function nodeStatusLabel(node: GraphTreeNode): string {
  const { execution, blockedOn, waitingForSlot } = node;
  const queued = blockedOn.length > 0 || waitingForSlot;
  switch (execution.state) {
    case "draft":
      return queued ? "queued" : "not started";
    case "preparing":
      return "starting";
    case "ready":
      return queued ? "queued" : "ready";
    case "working":
      return "working";
    case "needsInput":
      return execution.conflict ? "conflict" : "waiting";
    case "finished":
      return "done";
    case "failed":
      return "failed";
    case "stopped":
      return "stopped";
    case "missingSource":
      return "source missing";
    case "interrupted":
      return "stopped early";
  }
}

/** `.ag-node-dot`'s tone class family: `lead | done | working | waiting`, or
 * `undefined` for the neutral/queued dot. */
export function nodeDotTone(
  node: GraphTreeNode,
):
  | "lead"
  | "done"
  | "working"
  | "waiting"
  | "attention"
  | "interrupted"
  | undefined {
  if (node.isLead) return "lead";
  const label = nodeStatusLabel(node);
  if (label === "done") return "done";
  if (label === "working" || label === "starting") return "working";
  if (label === "waiting" || label === "conflict") return "waiting";
  // A node that needs a person must never be quieter than one that is fine.
  if (label === "failed" || label === "source missing") return "attention";
  // `stopped` and `stopped early` share a tone: both mean this agent ended
  // before finishing its work. The difference between them (one chosen, one
  // not) is carried by the label text beside the dot.
  //
  // `stopped` was missing here entirely, so it fell through to `undefined`
  // and drew the faint neutral dot used for "not started" -- a helper the
  // person stopped on purpose looked exactly like one that had not begun.
  // The text table in `AgentGraphPanel` handles `stopped` deliberately; only
  // this one, sitting beside it, was left out.
  if (label === "stopped" || label === "stopped early") return "interrupted";
  return undefined;
}

export function isNodeActive(execution: ExecutionRecord): boolean {
  return isActive(execution.state);
}

/** Summary text for the panel header, mockup `.ag-panel-summary` ("2
 * working · 1 waiting"). */
export function graphSummary(all: ExecutionRecord[]): string {
  // The lead's own review turn is not a fourth agent. It is created with the
  // lead as its parent and starts `Preparing`, so counting it made the header
  // read "3 working" for a two-helper run under review -- on the one line a
  // person glances at to decide whether the run still needs them.
  const reviewIds = new Set(
    all
      .map((e) => e.reviewExecutionId)
      .filter((id): id is string => id != null),
  );
  const executions = all.filter((e) => !reviewIds.has(e.executionId));
  const working = executions.filter(
    (e) => e.state === "working" || e.state === "preparing",
  ).length;
  const waiting = executions.filter((e) => e.state === "needsInput").length;
  // Counted so a dead helper cannot hide behind its peers: a graph of three
  // where one failed used to summarise as "2 working", which is true and
  // materially misleading -- the header is what a person glances at to decide
  // whether the run still needs them.
  // Was a fourth hand-written copy of this rule, in the file that already
  // imports the module exporting it -- and this one drives the count a person
  // glances at to decide whether a run still needs them.
  const stuck = executions.filter((e) => runStoppedBadly(e.state)).length;
  const parts: string[] = [];
  if (working > 0) parts.push(`${working} working`);
  if (waiting > 0) parts.push(`${waiting} waiting`);
  if (stuck > 0) parts.push(`${stuck} stopped`);
  // A run with nothing live still says how many agents it has, so a finished
  // graph reads "3 agents" rather than going blank -- and a graph whose only
  // notable state is a stopped agent still says the total alongside it, since
  // "1 stopped" alone loses how many there were.
  const total = `${executions.length} agent${executions.length === 1 ? "" : "s"}`;
  if (parts.length === 0) return total;
  if (working === 0 && waiting === 0) return `${total} · ${parts.join(" · ")}`;
  return parts.join(" · ");
}
