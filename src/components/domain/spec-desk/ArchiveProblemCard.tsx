import { useEffect, useState } from "react";
import { AlertTriangle, Wrench } from "lucide-react";
import { toast } from "sonner";
import type { ArchiveDiagnosis, CliOutcome } from "@/lib/bindings";
import { commands } from "@/lib/bindings";
import { unwrap } from "@/lib/queryKeys";
import { describeError } from "@/lib/log";

/**
 * Why an archive did not go through, and what to do about it.
 *
 * Inline rather than a toast: this is something to read and act on, and a toast
 * is gone in four seconds. The tool's own transcript is kept behind a
 * disclosure, because it is occasionally the thing a developer needs and always
 * the thing a beginner should not have to parse.
 *
 * When GitWyrm can fix the problem it says exactly what the fix would do before
 * the user accepts it. When it cannot, it says so rather than offering a button
 * that guesses -- a repair that moved someone's requirement into the wrong spec
 * would be far worse than the failure it replaced.
 */
export function ArchiveProblemCard({
  repoId,
  changeId,
  outcome,
  onArchived,
}: {
  repoId: string;
  changeId: string;
  outcome: CliOutcome & { kind: "failed" };
  onArchived: () => void;
}) {
  const [diagnosis, setDiagnosis] = useState<ArchiveDiagnosis | null>(null);
  const [fixing, setFixing] = useState(false);

  useEffect(() => {
    let cancelled = false;
    void (async () => {
      try {
        const d = unwrap(
          await commands.openspecDiagnoseArchive(changeId, outcome.output),
        );
        if (!cancelled) setDiagnosis(d);
      } catch {
        // A diagnosis that cannot be produced is not worth an error of its own;
        // the tool's output below still tells the user what happened.
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [changeId, outcome.output]);

  const applyFix = async () => {
    if (!diagnosis || fixing) return;
    setFixing(true);
    try {
      const result = unwrap(
        await commands.openspecRepairArchive(repoId, changeId, diagnosis.problem),
      );
      if (result.kind === "ok") {
        toast.success(`Archived ${changeId}.`);
        onArchived();
        return;
      }
      // Fixed the thing it knew about and still could not archive: show the new
      // failure rather than claiming success.
      toast.error("That fixed the folder, but the archive still did not go through.");
      setDiagnosis(
        unwrap(
          await commands.openspecDiagnoseArchive(
            changeId,
            result.kind === "failed" ? result.output : "",
          ),
        ),
      );
    } catch (e) {
      toast.error("Could not fix that.", { description: describeError(e) });
    } finally {
      setFixing(false);
    }
  };

  return (
    <div className="mt-2 rounded-md border border-[var(--gw-amber)]/40 bg-[var(--gw-amber)]/8 px-3 py-2.5 text-2xs leading-relaxed">
      <div className="flex items-start gap-1.5">
        <AlertTriangle
          size={12}
          strokeWidth={2.2}
          className="mt-px flex-none text-[var(--gw-amber)]"
        />
        <p className="font-semibold text-[var(--gw-amber)]">
          {diagnosis?.summary ?? `${changeId} could not be archived.`}
        </p>
      </div>

      {diagnosis && (
        <p className="mt-1.5 text-[var(--gw-amber)]/85">{diagnosis.detail}</p>
      )}

      {diagnosis?.fix_label && (
        <>
          <p className="mt-2 text-[var(--gw-amber)]/85">{diagnosis.fix_detail}</p>
          <button
            type="button"
            onClick={() => void applyFix()}
            disabled={fixing}
            className="mt-2 flex items-center gap-1.5 rounded border border-[var(--gw-amber)]/50 px-2 py-1 font-semibold text-[var(--gw-amber)] hover:bg-[var(--gw-amber)]/12 disabled:opacity-50"
          >
            <Wrench size={11} strokeWidth={2.3} />
            {fixing ? "Fixing…" : diagnosis.fix_label}
          </button>
        </>
      )}

      {diagnosis && !diagnosis.fix_label && (
        <p className="mt-2 text-[var(--gw-amber)]/85">
          This one needs a person: GitWyrm will not guess where the requirement belongs,
          because putting it in the wrong place would be harder to notice than this
          message.
        </p>
      )}

      {outcome.output.trim() && (
        <details className="group mt-2">
          <summary className="cursor-pointer list-none font-semibold text-[var(--gw-amber)]/70 hover:text-[var(--gw-amber)]">
            <span className="inline-block transition-transform group-open:rotate-90">
              ▸
            </span>{" "}
            What the OpenSpec tool said
          </summary>
          <pre className="mt-1.5 max-h-40 overflow-auto whitespace-pre-wrap font-mono text-2xs text-[var(--gw-amber)]/75">
            {outcome.output.trim()}
          </pre>
        </details>
      )}
    </div>
  );
}
