import { useEffect, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import {
  Archive,
  CheckCircle2,
  ClipboardCopy,
  Code2,
  MessageCircleQuestion,
  Play,
  Sparkles,
  SquareTerminal,
} from "lucide-react";
import { toast } from "sonner";
import { cn } from "@/lib/utils";
import { DisabledHint } from "@/components/ui/tooltip";
import type {
  ArchiveAttempt,
  CliOutcome,
  DemoScenario,
  DraftedArtifact,
  SpecChange,
} from "@/lib/bindings";
import { commands } from "@/lib/bindings";
import { unwrap } from "@/lib/queryKeys";
import { openAiSettings } from "@/lib/openAiSettings";
import { composeTaskHandoff, copyTaskHandoff } from "@/lib/specHandoff";
import { nextTask, useOpenspecMutations } from "@/hooks/useOpenspec";
import { useSpecAi } from "@/hooks/useSpecAi";
import { useAiSelection } from "@/hooks/useAiSelection";
import { isActive, stateGlyph, stateLabel, useAiRun } from "@/hooks/useAiRun";
import { useAskStore } from "@/stores/askStore";
import { useStartRun } from "@/hooks/useStartRun";
import { ConfirmDialog } from "@/components/modals/ConfirmDialog";
import { FormDialog } from "@/components/ui/form-dialog";
import { describeError, log } from "@/lib/log";
import { ArchiveProblemCard } from "./ArchiveProblemCard";

/**
 * Why the opencode button is off. Names the fix rather than the failure: the
 * button being grey is the symptom the user can already see.
 */
const OPENCODE_MISSING =
  "opencode is not installed. Get it from opencode.ai - this button turns on once it is.";

/** A button in the rail. Primary is the one action the state calls for. */
function RailButton({
  icon,
  label,
  onClick,
  primary,
  pending,
  disabled,
  title,
}: {
  icon?: React.ReactNode;
  label: string;
  onClick: () => void;
  primary?: boolean;
  pending?: boolean;
  disabled?: boolean;
  /** Shown on hover, so a disabled button can say why. */
  title?: string;
}) {
  const button = (
    <button
      type="button"
      onClick={onClick}
      disabled={pending || disabled}
      className={cn(
        "flex min-h-8 w-full items-center justify-center gap-2 rounded-md px-3 text-2xs font-semibold transition-colors disabled:opacity-50",
        primary
          ? "bg-primary text-primary-foreground hover:brightness-110"
          : "border border-border bg-panel2 text-foreground hover:border-muted-foreground hover:bg-panel3",
      )}
    >
      {icon}
      {pending ? "Working…" : label}
    </button>
  );

  // Deliberately not the `title` attribute: that draws the OS tooltip, which
  // ignores the app's theme and takes a second to appear. DisabledHint puts the
  // reason on a hoverable wrapper, since a disabled button gets no pointer
  // events and could never show a tooltip of its own.
  return (
    <DisabledHint disabled={disabled} reason={title} className="w-full">
      {button}
    </DisabledHint>
  );
}

/**
 * The inline result of a spec check.
 *
 * Deliberately not a toast: a toast is gone in four seconds, and the whole point
 * of a check is to sit there while the user fixes what it found. It stays until
 * the change is switched or the check is run again.
 */
function CheckResult({
  outcome,
  onRecheck,
  onFix,
  fixing,
}: {
  outcome: CliOutcome;
  onRecheck: () => void;
  /** Offered only with an AI configured; absent otherwise. */
  onFix?: () => void;
  fixing?: boolean;
}) {
  if (outcome.kind === "cliMissing") {
    return (
      <div className="mt-2 rounded-md border border-[var(--gw-amber)]/40 bg-[var(--gw-amber)]/8 px-3 py-2 text-2xs leading-relaxed text-[var(--gw-amber)]">
        <p className="font-semibold">The OpenSpec tool is not installed.</p>
        <p className="mt-1 text-[var(--gw-amber)]/85">{outcome.hint}</p>
        {/* Installing it is the expected next step, and the probe is cached, so
            without this the panel would keep saying "not installed" until the
            app restarted. */}
        <button
          type="button"
          onClick={onRecheck}
          className="mt-2 rounded border border-[var(--gw-amber)]/50 px-2 py-1 text-2xs font-semibold text-[var(--gw-amber)] hover:bg-[var(--gw-amber)]/12"
        >
          I installed it - check again
        </button>
      </div>
    );
  }
  if (outcome.kind === "failed") {
    return (
      <div className="mt-2 rounded-md border border-[var(--gw-red)]/40 bg-[var(--gw-red)]/8 px-3 py-2 text-2xs leading-relaxed">
        <p className="font-semibold text-removed">
          This change has problems to fix.
        </p>
        {outcome.output.trim() && (
          <pre className="mt-1.5 max-h-40 overflow-auto whitespace-pre-wrap font-mono text-2xs text-sub">
            {outcome.output.trim()}
          </pre>
        )}
        {onFix && (
          <button
            type="button"
            onClick={onFix}
            disabled={fixing}
            className="mt-2 flex items-center gap-1.5 rounded border border-primary/45 px-2 py-1 text-2xs font-semibold text-accent-text hover:bg-primary/15 disabled:opacity-50"
          >
            <Sparkles size={11} strokeWidth={2.4} />
            {fixing ? "Drafting…" : "Fix with AI"}
          </button>
        )}
      </div>
    );
  }
  return (
    <div className="mt-2 rounded-md border border-primary/40 bg-soft px-3 py-2 text-2xs leading-relaxed">
      <p className="font-semibold text-accent-text">The spec check passed.</p>
      <p className="mt-1 text-sub">
        The proposal, tasks, and requirements are all shaped the way OpenSpec
        expects.
      </p>
      {outcome.output.trim() && (
        <pre className="mt-1.5 max-h-40 overflow-auto whitespace-pre-wrap font-mono text-2xs text-muted-foreground">
          {outcome.output.trim()}
        </pre>
      )}
    </div>
  );
}

/**
 * The Desk's right column: hand this change's next task to whatever tool the
 * user prefers, check it, and archive it when it is done.
 *
 * Everything here works with no AI configured -- copy a handoff and paste it
 * anywhere. `add-ai-provider-surface` later reorders this card to put an in-app
 * run first, and moves these into a disclosure; nothing here is replaced.
 */
export function DeskActionRail({
  change,
  repoId,
  repoPath,
}: {
  change: SpecChange;
  repoId: string;
  repoPath: string;
}) {
  const ai = useSpecAi();
  const { validateChange, archiveChange, draftFix, addDraftedDelta } =
    useOpenspecMutations(repoId);
  const selection = useAiSelection();
  const run = useAiRun(repoId);
  const startAskSession = useAskStore((s) => s.start);
  const [result, setResult] = useState<CliOutcome | null>(null);
  // Which change `result` describes. Not always the selected one: the user can
  // switch changes while a fix drafts, and the fix belongs to what was checked.
  const [checkedId, setCheckedId] = useState<string | null>(null);
  const [fix, setFix] = useState<{
    changeId: string;
    delta: DraftedArtifact;
  } | null>(null);
  const { startRun, starting } = useStartRun(repoId, change);
  /** Run the task in its own copy of the project. Off by default. */
  const [ownFolder, setOwnFolder] = useState(false);
  const [confirmArchive, setConfirmArchive] = useState(false);
  // A failed archive, with the change it belongs to. Carries its own id for the
  // same reason the drafted fix does: the user can select elsewhere while
  // reading it, and the repair must land on what they actually archived.
  const [archiveProblem, setArchiveProblem] = useState<{
    changeId: string;
    outcome: CliOutcome & { kind: "failed" };
  } | null>(null);
  // Set when GitWyrm stopped before archiving because the change is not
  // finished. Nothing has run at this point, so this is a question, not a
  // report.
  const [archiveBlock, setArchiveBlock] = useState<
    (ArchiveAttempt & { kind: "blocked" }) | null
  >(null);
  // Held here, not inside the <details>: the rail re-renders on every task tick
  // and watcher refresh, and a details element rebuilt from JSX would snap shut
  // under the user mid-read.
  const [handoffOpen, setHandoffOpen] = useState(false);
  const [inviteDismissed, setInviteDismissed] = useState(false);

  const task = nextTask(change);
  const allDone = !task && !change.progress.is_draft;
  const remaining = change.progress.total - change.progress.done;
  const handoff = composeTaskHandoff(change, task);

  // Whether opencode can actually be launched. A machine-level fact, so it is
  // not keyed by repo. Assumed available until the probe answers, so the button
  // does not flicker disabled on open.
  //
  // Re-probed when the window regains focus rather than cached for minutes:
  // installing opencode is exactly what someone does after seeing the button
  // disabled, and they come back to a window that should have noticed. The
  // probe is a `where`/`which` lookup, so this is cheap to repeat.
  const opencodeReady = useQuery({
    queryKey: ["opencodeAvailable"],
    queryFn: () => commands.opencodeAvailable(),
    staleTime: 10_000,
    refetchOnWindowFocus: true,
  });
  const opencodeMissing = opencodeReady.data === false;

  // A result belongs to the change it was run on. Switching changes has to clear
  // it, or the rail would show one change's problems beside another's name.
  // The drafted fix is deliberately NOT cleared: it names the change it belongs
  // to and writes there, so a user who switches while it drafts still gets to
  // decide about it rather than losing it silently.
  useEffect(() => {
    setResult(null);
    setCheckedId(null);
    // The archive problem goes too: it names its own change, and leaving it
    // under another change's header would read as a problem with that one.
    setArchiveProblem(null);
    setArchiveBlock(null);
  }, [change.id]);

  const runCheck = () => {
    // Remember which change this result belongs to. "Fix with AI" must land on
    // the change that was checked even if the user selects another one while
    // the fix drafts, and the result panel outlives the selection.
    const checked = change.id;
    validateChange.mutate(change.id, {
      onSuccess: (outcome) => {
        setResult(outcome);
        setCheckedId(checked);
      },
      onError: (e) => toast.error(describeError(e)),
    });
  };

  // Ask and runs share the ✦ tab and the one-session-at-a-time rule, so a
  // working run holds the tab. `isActive` covers the states where a run is
  // mid-flight or waiting on the user, not one that has already ended.
  const runIsActive = isActive(run.state);

  const startAsk = () => {
    if (runIsActive) {
      toast.info("A run is working right now.", {
        description: "Open the ✦ tab to watch it, then ask once it finishes.",
      });
      return;
    }
    startAskSession(repoId, change.id);
    toast.success("Ask is open in the ✦ tab.", {
      description: "It reads this change and the code, and changes nothing.",
    });
  };

  const startFix = () => {
    const target = checkedId;
    if (!target || !result || result.kind !== "failed") return;
    if (!selection.provider || !selection.model) return;
    draftFix.mutate(
      {
        changeId: target,
        validatorOutput: result.output,
        provider: selection.provider,
        model: selection.model,
      },
      {
        onSuccess: (delta) => setFix({ changeId: target, delta }),
        onError: (e) =>
          toast.error("Could not draft a fix.", {
            description: describeError(e),
          }),
      },
    );
  };

  const applyFix = () => {
    if (!fix) return;
    addDraftedDelta.mutate(
      { changeId: fix.changeId, delta: fix.delta },
      {
        onSuccess: (path) => {
          toast.success("Added the requirement.", {
            description: `Wrote ${path}.`,
          });
          setFix(null);
          // Re-run the check on the change the fix landed on, not on whatever is
          // selected now, so the result the user sees is about their fix.
          validateChange.mutate(fix.changeId, {
            onSuccess: (outcome) => {
              setResult(outcome);
              setCheckedId(fix.changeId);
            },
          });
        },
        onError: (e) =>
          toast.error("Could not add the requirement.", {
            description: describeError(e),
          }),
      },
    );
  };

  // Re-probe for the CLI, then immediately re-run the check if it turned up.
  // Two clicks to get from "not installed" back to a result would be one too
  // many when the user has already done the installing.
  const recheckCli = async () => {
    try {
      const info = unwrap(await commands.openspecRecheckCli());
      if (!info.available) {
        toast.info("Still cannot find the OpenSpec tool.", {
          description:
            "If you just installed it, opening a new terminal may be needed first.",
        });
        return;
      }
      toast.success(
        `Found the OpenSpec tool${info.version ? ` (${info.version})` : ""}.`,
      );
      runCheck();
    } catch (e) {
      log.error(`spec desk: opencode cli recheck failed: ${describeError(e)}`);
      toast.error("Could not check for the OpenSpec tool.", {
        description: describeError(e),
      });
    }
  };

  const archive = (force = false) => {
    archiveChange.mutate(
      { changeId: change.id, force },
      {
        onSuccess: (attempt) => {
          // GitWyrm stopped before running anything. Nothing merged, nothing
          // moved, nothing committed -- the user gets to decide with the facts.
          if (attempt.kind === "blocked") {
            setConfirmArchive(false);
            setArchiveBlock(attempt);
            return;
          }
          const outcome = attempt.outcome;
          if (outcome.kind === "ok") {
            toast.success(`Archived ${change.id}.`, {
              description: "Its requirements are part of your specs now.",
            });
            setConfirmArchive(false);
            setArchiveBlock(null);
            return;
          }
          // Close the dialog and put the problem in the rail. Archive failures
          // get their own panel because most of them are fixable, and a toast
          // cannot hold an explanation plus the button that acts on it.
          setConfirmArchive(false);
          if (outcome.kind === "failed") {
            setArchiveProblem({ changeId: change.id, outcome });
            return;
          }
          // cliMissing: nothing to diagnose, the tool was never there.
          setResult(outcome);
        },
        onError: (e) => toast.error(describeError(e)),
      }
    );
  };

  const openInOpencode = async () => {
    // The handoff goes to opencode as its opening message, so there is nothing
    // to paste. It is copied anyway: if the launch fails halfway, or the user
    // wants it in a second window, it is already in hand.
    await copyTaskHandoff(change, task, { silent: true });
    try {
      unwrap(await commands.openInOpencode(repoId, handoff));
      toast.success("opencode is starting on this task.", {
        description: "The handoff is already in the conversation.",
      });
    } catch (e) {
      log.error(`spec desk: open in opencode failed: ${describeError(e)}`);
      toast.error("Could not start opencode.", {
        description: `${describeError(e)} The handoff is on your clipboard.`,
      });
    }
  };

  const openInVsCode = async () => {
    await copyTaskHandoff(change, task, { silent: true });
    try {
      unwrap(await commands.openInEditor(repoId, "vs_code"));
      toast.success("VS Code opened with the handoff copied.");
    } catch (e) {
      log.error(`spec desk: open in editor failed: ${describeError(e)}`);
      toast.error("Could not open VS Code.", { description: describeError(e) });
    }
  };

  return (
    <div className="flex min-h-0 flex-col border-l border-border bg-panel">
      <header className="flex-none border-b border-border px-4 py-3.5">
        <h2 className="text-sm font-semibold text-foreground">
          Next best action
        </h2>
        <p className="mt-0.5 text-2xs text-muted-foreground">
          Work from the spec. Keep the tools you already use.
        </p>
      </header>

      <div className="min-h-0 flex-1 overflow-y-auto p-3.5">
        <RunBanner repoId={repoId} />
        <section className="rounded-lg border border-primary/25 bg-soft px-3.5 py-3">
          <p className="text-2xs font-bold tracking-[.09em] text-accent-text">
            {task
              ? `NEXT TASK · ${change.progress.done + 1} OF ${change.progress.total}`
              : allDone
                ? "EVERY TASK IS DONE"
                : "NO TASKS YET"}
          </p>
          <h3 className="mt-1.5 text-xs font-semibold leading-snug text-foreground">
            {task
              ? task.text
              : allDone
                ? "Check it, then archive it"
                : "Add tasks to tasks.md to get started"}
          </h3>
          <p className="mt-1.5 text-2xs leading-relaxed text-sub">
            {ai.configured
              ? `${ai.providerShort} reads this change's proposal and agreed behavior first, then does just this task. You watch every step and can stop anytime.`
              : "The handoff carries this change's proposal, its agreed behavior, and the one task - so whatever you paste it into does not have to guess."}
          </p>

          <div className="mt-3 flex flex-col gap-2">
            {ai.configured ? (
              <>
                {task && (
                  <>
                    <RailButton
                      primary
                      icon={<Play size={12} strokeWidth={2.6} />}
                      label={starting ? "Starting…" : "Run this task with AI"}
                      onClick={() => void startRun(ownFolder)}
                    />
                    {/* Off by default. A run already leaves the user's work
                        untouched by setting it aside; a folder is only worth
                        its cleanup cost when they want to keep editing the
                        same files while it works. */}
                    <label className="flex cursor-pointer items-start gap-1.5 px-0.5 text-2xs leading-relaxed text-sub hover:text-foreground">
                      <input
                        type="checkbox"
                        checked={ownFolder}
                        onChange={(e) => setOwnFolder(e.target.checked)}
                        className="mt-0.5 size-3 flex-none accent-[var(--gw-accent)]"
                      />
                      <span>
                        Run it in its own worktree so you can keep editing
                        <span className="block text-muted-foreground">
                          Checks this project out into a separate folder for the task, then shows
                          you what changed before anything reaches your branch.
                        </span>
                      </span>
                    </label>
                  </>
                )}
                {/* Ask shares the ✦ tab with runs, and only one session exists at
                    a time, so this is off while a run is working rather than
                    quietly replacing what the user is watching. */}
                <RailButton
                  icon={<MessageCircleQuestion size={12} strokeWidth={2.2} />}
                  label="Ask about this change"
                  disabled={runIsActive}
                  title={
                    runIsActive
                      ? "A run is working right now. Open the ✦ tab to watch it, then ask when it finishes."
                      : undefined
                  }
                  onClick={startAsk}
                />
              </>
            ) : (
              <>
                <RailButton
                  primary
                  icon={<ClipboardCopy size={12} strokeWidth={2.4} />}
                  label={allDone ? "Copy review handoff" : "Copy task handoff"}
                  onClick={() => void copyTaskHandoff(change, task)}
                />
                <RailButton
                  icon={<SquareTerminal size={12} strokeWidth={2.2} />}
                  label="Open in opencode"
                  disabled={opencodeMissing}
                  title={opencodeMissing ? OPENCODE_MISSING : undefined}
                  onClick={() => void openInOpencode()}
                />
                <RailButton
                  icon={<Code2 size={12} strokeWidth={2.2} />}
                  label="Open in VS Code"
                  onClick={() => void openInVsCode()}
                />
              </>
            )}
          </div>

          {ai.configured && (
            <p className="mt-2.5 text-2xs leading-relaxed text-muted-foreground">
              Runs with{" "}
              <span className="font-semibold text-sub">{ai.provider}</span>
              {ai.model && <> · {ai.model}</>} · uses your {ai.providerShort}{" "}
              plan
            </p>
          )}
        </section>

        {/* With an AI configured this drops to a disclosure: still one click away
            for anyone who lives in opencode, but no longer the first thing. With
            no AI it is the whole workflow and stays open. */}
        {ai.configured ? (
          <details
            className="group mt-4"
            open={handoffOpen}
            onToggle={(e) =>
              setHandoffOpen((e.currentTarget as HTMLDetailsElement).open)
            }
          >
            <summary className="cursor-pointer list-none text-2xs font-bold tracking-[.1em] text-sub hover:text-foreground">
              <span className="inline-block transition-transform group-open:rotate-90">
                ▸
              </span>{" "}
              PREFER YOUR OWN EDITOR?
            </summary>
            <p className="mt-2 text-2xs leading-relaxed text-muted-foreground">
              Copy this task to opencode, VS Code, or any AI chat - the same
              handoff, your tool.
            </p>
            <div className="mt-2 flex flex-col gap-2">
              <RailButton
                icon={<ClipboardCopy size={12} strokeWidth={2.4} />}
                label={allDone ? "Copy review handoff" : "Copy task handoff"}
                onClick={() => void copyTaskHandoff(change, task)}
              />
              <RailButton
                icon={<SquareTerminal size={12} strokeWidth={2.2} />}
                label="Open in opencode"
                disabled={opencodeMissing}
                title={opencodeMissing ? OPENCODE_MISSING : undefined}
                onClick={() => void openInOpencode()}
              />
              <RailButton
                icon={<Code2 size={12} strokeWidth={2.2} />}
                label="Open in VS Code"
                onClick={() => void openInVsCode()}
              />
            </div>
            <h3 className="mb-1.5 mt-3 text-2xs font-bold tracking-[.1em] text-sub">
              WHAT THE AI READS
            </h3>
            <pre className="max-h-56 overflow-auto whitespace-pre-wrap rounded-md border border-border bg-background px-3 py-2.5 font-mono text-2xs leading-relaxed text-sub">
              {handoff}
            </pre>
          </details>
        ) : (
          <section className="mt-4">
            <h3 className="mb-1.5 text-2xs font-bold tracking-[.1em] text-sub">
              WHAT GETS COPIED
            </h3>
            <pre className="max-h-56 overflow-auto whitespace-pre-wrap rounded-md border border-border bg-background px-3 py-2.5 font-mono text-2xs leading-relaxed text-sub">
              {handoff}
            </pre>
            <p className="mt-1.5 text-2xs text-muted-foreground">{repoPath}</p>
          </section>
        )}

        <section className="mt-4">
          <h3 className="mb-1.5 text-2xs font-bold tracking-[.1em] text-sub">
            CLOSE THE LOOP
          </h3>
          <div className="flex flex-col gap-2">
            <RailButton
              icon={<CheckCircle2 size={12} strokeWidth={2.2} />}
              label="Run spec check"
              onClick={runCheck}
              pending={validateChange.isPending}
            />
            {result && (
              <CheckResult
                outcome={result}
                onRecheck={() => void recheckCli()}
                onFix={ai.configured && checkedId ? startFix : undefined}
                fixing={draftFix.isPending}
              />
            )}
            {/* Only shown after the check, so it reads as "what next" rather than
                a warning about something the user has not done. */}
            <RailButton
              icon={<Archive size={12} strokeWidth={2.2} />}
              label="Archive this change…"
              onClick={() => {
                // Say why rather than offering a dead button: the count is the
                // actionable part.
                if (!allDone) {
                  toast.info(
                    change.progress.is_draft
                      ? "Add tasks and finish them before archiving this change."
                      : `${remaining} ${remaining === 1 ? "task is" : "tasks are"} still open.`,
                  );
                  return;
                }
                if (change.deltas.length === 0) {
                  toast.info(
                    "This change has no requirements yet, so there is nothing to archive into your specs.",
                  );
                  return;
                }
                setConfirmArchive(true);
              }}
            />
            {archiveBlock && (
              <div className="mt-2 rounded-md border border-[var(--gw-amber)]/40 bg-[var(--gw-amber)]/8 px-3 py-2.5 text-2xs leading-relaxed">
                <p className="font-semibold text-[var(--gw-amber)]">
                  {archiveBlock.summary}
                </p>
                <p className="mt-1.5 text-[var(--gw-amber)]/85">{archiveBlock.detail}</p>
                <p className="mt-1.5 text-[var(--gw-amber)]/85">
                  Nothing has been archived or committed.
                </p>
                <div className="mt-2 flex flex-wrap gap-1.5">
                  {/* The override is allowed -- the user knows things the app
                      does not -- but only after reading what it would do. */}
                  <button
                    type="button"
                    onClick={() => archive(true)}
                    disabled={archiveChange.isPending}
                    className="rounded border border-[var(--gw-amber)]/50 px-2 py-1 font-semibold text-[var(--gw-amber)] hover:bg-[var(--gw-amber)]/12 disabled:opacity-50"
                  >
                    {archiveChange.isPending ? "Archiving…" : "Archive it anyway"}
                  </button>
                  <button
                    type="button"
                    onClick={() => setArchiveBlock(null)}
                    className="rounded border border-border px-2 py-1 font-semibold text-muted-foreground hover:text-foreground"
                  >
                    Not yet
                  </button>
                </div>
              </div>
            )}
            {archiveProblem && (
              <ArchiveProblemCard
                repoId={repoId}
                changeId={archiveProblem.changeId}
                outcome={archiveProblem.outcome}
                onArchived={() => setArchiveProblem(null)}
              />
            )}
          </div>
        </section>

        {/* One quiet invitation, at the bottom, dismissible. Never a modal, never
            the rail's primary action: the copy workflow above is complete on its
            own, and a nag would imply otherwise. */}
        {ai.state === "none" && !inviteDismissed && (
          <section className="mt-4 rounded-lg border border-dashed border-border px-3.5 py-3">
            <h3 className="text-2xs font-bold tracking-[.09em] text-sub">
              RUN TASKS RIGHT HERE
            </h3>
            <p className="mt-1.5 text-2xs leading-relaxed text-muted-foreground">
              Connect the AI you already use - Copilot, Anthropic, or a local
              model - and GitWyrm can work on tasks in this window. You watch
              every step and can stop it anytime.
            </p>
            <div className="mt-2.5 flex items-center gap-2">
              <RailButton
                label="Connect an AI"
                onClick={() => void openAiSettings()}
              />
              <button
                type="button"
                onClick={() => setInviteDismissed(true)}
                className="flex-none px-1 text-2xs text-muted-foreground hover:text-sub"
              >
                Not now
              </button>
            </div>
          </section>
        )}

        {ai.state === "reconnect" && (
          <section className="mt-4 rounded-lg border border-[var(--gw-amber)]/40 bg-[var(--gw-amber)]/8 px-3.5 py-3">
            <h3 className="text-2xs font-bold tracking-[.09em] text-[var(--gw-amber)]">
              {ai.providerShort.toUpperCase()} NEEDS RECONNECTING
            </h3>
            <p className="mt-1.5 text-2xs leading-relaxed text-[var(--gw-amber)]/85">
              It is signed in but has no models available, so a run could not
              start. Reconnect it in Settings → AI. Copying handoffs works
              either way.
            </p>
          </section>
        )}
      </div>

      <ConfirmDialog
        open={confirmArchive}
        onOpenChange={setConfirmArchive}
        title={`Archive ${change.id}?`}
        description={
          <>
            Every task is done. Archiving folds this change's{" "}
            {change.deltas.length === 1 ? "requirement" : "requirements"} into
            your specs and moves it out of the active list. It stays on disk,
            under <span className="font-mono">openspec/changes/archive</span>.
          </>
        }
        confirmLabel="Archive it"
        pending={archiveChange.isPending}
        pendingLabel="Archiving…"
        onConfirm={() => archive()}
      />
      {/* The drafted fix, for review. Nothing is on disk until Add. */}
      <FormDialog
        open={fix != null}
        onOpenChange={(o) => !o && setFix(null)}
        icon={<Sparkles size={15} strokeWidth={1.9} />}
        widthClassName="sm:max-w-2xl"
        title={fix ? `Add a requirement to ${fix.changeId}` : ""}
        submitLabel="Add this delta"
        pendingLabel="Adding…"
        cancelLabel="Dismiss"
        canSubmit={!addDraftedDelta.isPending}
        pending={addDraftedDelta.isPending}
        onSubmit={applyFix}
      >
        <p className="text-2xs leading-relaxed text-muted-foreground">
          Drafted from this change's own proposal. Nothing is written until you
          add it.
        </p>
        {fix && (
          <>
            <p className="font-mono text-2xs text-sub">{fix.delta.path}</p>
            <pre className="max-h-72 overflow-auto whitespace-pre-wrap rounded border border-border bg-background px-2.5 py-2 font-mono text-2xs leading-relaxed text-sub">
              {fix.delta.content}
            </pre>
          </>
        )}
      </FormDialog>
    </div>
  );
}

/**
 * The current run, shown in the rail so a gate is visible while the Desk is on
 * another tab. Silent when nothing is running.
 */
function RunBanner({ repoId }: { repoId: string }) {
  const run = useAiRun(repoId);
  if (!run.session || !run.state) return null;
  const needsYou = run.state === "needsYou";
  return (
    <div
      className={cn(
        "mb-3 rounded-lg border px-3 py-2",
        needsYou
          ? "border-[var(--gw-amber)]/40 bg-[var(--gw-amber)]/8"
          : "border-border bg-panel2",
      )}
    >
      <div className="flex items-center gap-1.5">
        <span
          className={cn(
            "flex-none text-2xs",
            needsYou && "text-[var(--gw-amber)]",
          )}
        >
          {stateGlyph(run.state)}
        </span>
        <span
          className={cn(
            "min-w-0 flex-1 truncate text-2xs font-semibold",
            needsYou ? "text-[var(--gw-amber)]" : "text-sub",
          )}
        >
          {needsYou ? "This run needs you" : stateLabel(run.state)}
        </span>
      </div>
      <p className="mt-0.5 line-clamp-2 text-2xs text-muted-foreground">
        Task {run.session.task_number} · {run.session.task_text}
      </p>
    </div>
  );
}
