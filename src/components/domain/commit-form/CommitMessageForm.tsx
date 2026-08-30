import { useEffect, useRef, useState } from "react";
import {
  Check,
  GitCommitHorizontal,
  Pencil,
  Sparkles,
  Upload,
} from "lucide-react";
import { toast } from "sonner";
import logoUrl from "@/assets/logo.png";
import { cn } from "@/lib/utils";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import { ProfileSwitcher } from "@/components/domain/ProfileSwitcher";
import {
  DisabledHint,
  Tooltip,
  TooltipButton,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import { plural } from "@/lib/gitDisplay";
import { branchSync } from "@/lib/branchActions";
import { useAiMutations } from "@/hooks/useAi";
import { useAiSelection } from "@/hooks/useAiSelection";
import {
  useBranches,
  useCommitDetail,
  useCommitLog,
  useStatus,
} from "@/hooks/useGitQueries";
import { useGitMutations } from "@/hooks/useGitMutations";
import {
  useActiveRepo,
  useWorkspaceStore,
  DEFAULT_COMMIT_DESC_LINES,
  MAX_COMMIT_DESC_LINES,
  MIN_COMMIT_DESC_LINES,
  type CommitButtonMode,
} from "@/stores/workspaceStore";
import { ResizeHandle } from "@/components/ui/ResizeHandle";
import { useUiStore } from "@/stores/uiStore";

export function CommitMessageForm() {
  const repo = useActiveRepo();
  const status = useStatus(repo?.id ?? null);
  const branches = useBranches(repo?.id ?? null);
  const log = useCommitLog(repo?.id ?? null);
  const m = useGitMutations(repo?.id ?? null);
  // The draft lives in the store, keyed by repo, so switching tabs and coming
  // back returns to what was typed. The form is keyed by repo id, so this
  // instance only ever reads and writes its own repo's draft.
  const repoId = repo?.id ?? null;
  const draft = useWorkspaceStore((s) =>
    repoId ? s.commitDrafts[repoId] : undefined,
  );
  const setCommitDraft = useWorkspaceStore((s) => s.setCommitDraft);
  const clearCommitDraft = useWorkspaceStore((s) => s.clearCommitDraft);
  const msg = draft?.summary ?? "";
  const desc = draft?.description ?? "";
  const amend = draft?.amend ?? false;
  const setMsg = (summary: string) => {
    if (repoId) setCommitDraft(repoId, { summary });
  };
  const setDesc = (description: string) => {
    if (repoId) setCommitDraft(repoId, { description });
  };
  const [justGenerated, setJustGenerated] = useState(false);
  const generatedTimer = useRef<number | null>(null);
  const descRef = useRef<HTMLTextAreaElement | null>(null);

  // The handle moves the box in lines, but the pointer moves in pixels, and a
  // line is not a fixed height: the Text Size setting scales the root font, and
  // the UI Scale setting zooms the whole body. Measuring the box itself covers
  // both, so the edge tracks the pointer at any setting.
  const pixelsToLines = (pixels: number) => {
    const el = descRef.current;
    const lineHeight = el
      ? Number.parseFloat(getComputedStyle(el).lineHeight)
      : Number.NaN;
    return pixels / (Number.isFinite(lineHeight) && lineHeight > 0 ? lineHeight : 16);
  };

  const headSha = log.data?.pages[0]?.commits[0]?.sha ?? null;
  const headDetail = useCommitDetail(repo?.id ?? null, amend ? headSha : null);

  const ai = useAiMutations();
  const showSettings = useUiStore((s) => s.showSettings);
  const openModal = useUiStore((s) => s.openModal);
  const commitButtonMode = useWorkspaceStore((s) => s.commitButtonMode);
  const commitDescLines = useWorkspaceStore((s) => s.commitDescLines);
  const setCommitDescLines = useWorkspaceStore((s) => s.setCommitDescLines);
  const setCommitButtonMode = useWorkspaceStore((s) => s.setCommitButtonMode);
  // One resolver, shared with the commit-generation dialog and the Spec Desk, so
  // they cannot disagree about which AI is in use.
  const { ready: aiReady, provider: aiProvider, model: aiModel } = useAiSelection();

  const stagedCount = status.data?.staged.length ?? 0;
  const currentBranch =
    branches.data?.local.find((b) => b.is_head)?.name ?? "HEAD";
  const pushPending = m.push.isPending;
  const canAmend = headSha != null;
  // Amend can change just the message, so it does not require staged files.
  const hasWork = amend ? canAmend : stagedCount > 0;
  const canCommit =
    hasWork &&
    msg.trim().length > 0 &&
    !m.createCommit.isPending &&
    !pushPending &&
    !ai.generate.isPending;

  useEffect(
    () => () => {
      if (generatedTimer.current != null)
        window.clearTimeout(generatedTimer.current);
    },
    [],
  );

  // Amend swaps the box between two jobs: writing a new commit and rewriting
  // the previous one. The fields always follow the toggle -- turning it on
  // shows the previous message (replacing anything typed), turning it off
  // clears back to an empty new commit.
  const toggleAmend = (next: boolean) => {
    if (!repoId) return;
    if (!next) {
      setCommitDraft(repoId, { amend: false, summary: "", description: "" });
      return;
    }
    // The previous commit may not be loaded yet on the first tick; the effect
    // below fills it in as soon as it arrives.
    setCommitDraft(repoId, {
      amend: true,
      summary: headDetail.data?.summary ?? "",
      description: headDetail.data?.body ?? "",
    });
  };

  // Covers the case above: amend was switched on before the previous commit
  // had loaded. Only fills empty fields, so it never clobbers an edit the user
  // started while the fetch was in flight.
  useEffect(() => {
    if (!amend || !headDetail.data || !repoId) return;
    const current = useWorkspaceStore.getState().commitDrafts[repoId];
    setCommitDraft(repoId, {
      summary: current?.summary.trim() ? current.summary : headDetail.data.summary,
      description: current?.description.trim()
        ? current.description
        : headDetail.data.body,
    });
  }, [amend, headDetail.data, repoId, setCommitDraft]);

  // Why the commit button is off, shown on hover. The button is disabled in
  // exactly these cases, so this is the only way the user learns what to fix.
  const commitBlockedReason = m.createCommit.isPending
    ? "Committing…"
    : pushPending
      ? "Pushing…"
      : ai.generate.isPending
        ? "Waiting for the generated message"
        : amend && !canAmend
          ? "No commit to amend"
          : !hasWork
            ? "Stage files to commit"
            : msg.trim().length === 0
              ? "Enter a commit message"
              : undefined;

  const doCommit = (mode: CommitButtonMode = commitButtonMode) => {
    if (!canCommit) return;
    m.createCommit.mutate(
      { summary: msg, description: desc, amend, specId: null },
      {
        onSuccess: async () => {
          if (repoId) clearCommitDraft(repoId);
          if (mode !== "commit_push") return;
          // Amending a commit the remote already has leaves the branch
          // diverged (behind > 0), so a plain push would be rejected. Refetch
          // the sync state and route through the force-push choice, matching
          // the toolbar Push button, instead of firing a push we know fails.
          const fresh = await branches.refetch();
          const head = fresh.data?.local.find((b) => b.is_head);
          const behind = head ? branchSync(head).behind : 0;
          if (behind > 0) openModal("push-choice");
          else m.push.mutate();
        },
      },
    );
  };

  const doGenerate = () => {
    if (!aiReady) {
      showSettings("ai");
      return;
    }
    if (ai.generate.isPending || !repo || stagedCount === 0) return;
    const forRepo = repo.id;
    setJustGenerated(false);
    ai.generate.mutate(
      { repoId: forRepo, provider: aiProvider!, model: aiModel! },
      {
        onSuccess: (r) => {
          // The user can switch tabs while this is in flight. The draft is
          // keyed by repo, so the message always lands on the repo it was
          // generated from -- waiting there if they have moved on.
          setCommitDraft(forRepo, {
            summary: r.summary,
            description: r.description,
          });
          if (useWorkspaceStore.getState().activeRepoId !== forRepo) return;
          setJustGenerated(true);
          if (generatedTimer.current != null)
            window.clearTimeout(generatedTimer.current);
          generatedTimer.current = window.setTimeout(
            () => setJustGenerated(false),
            1400,
          );
        },
        onError: (e) => {
          if (useWorkspaceStore.getState().activeRepoId !== forRepo) return;
          toast.error(String(e));
        },
      },
    );
  };

  const generating = ai.generate.isPending;
  const nothingToGenerateFrom = aiReady && (!repo || stagedCount === 0);

  return (
    <div
      className="relative flex-none border-t border-border bg-panel2 px-3 pb-[13px] pt-[11px]"
      style={
        {
          "--wyrm-commit-desc-lines": commitDescLines,
        } as React.CSSProperties
      }
    >
      {/*
        Sits on the panel's top edge: dragging up gives the description more
        room and hands the space back to the file list below it. One line of
        text per 1rem of travel, so the box follows the pointer.
      */}
      <ResizeHandle
        ariaLabel="Resize commit message box"
        axis="y"
        direction={-1}
        value={commitDescLines}
        min={MIN_COMMIT_DESC_LINES}
        max={MAX_COMMIT_DESC_LINES}
        defaultValue={DEFAULT_COMMIT_DESC_LINES}
        onChange={setCommitDescLines}
        toValue={pixelsToLines}
        className="-top-1"
      />
      <div className={cn("relative mb-[9px]", generating && "min-h-[116px]")}>
        <div className="relative mb-[7px] rounded-md">
          <Input
            value={generating ? "" : msg}
            onChange={(e) => setMsg(e.target.value)}
            disabled={generating}
            placeholder={generating ? "" : "Summary (required)"}
            className={cn(
              "h-auto bg-background py-2 pl-2.5 pr-9 text-xs transition-[opacity,filter] duration-200",
              generating && "opacity-[0.18] saturate-[0.35]",
            )}
          />
          {/* The wrapper takes over the button's absolute placement: a
              `display:contents` span would have no box to hover, leaving the
              hint unreachable. */}
          <DisabledHint
            disabled={nothingToGenerateFrom}
            reason="Stage files to generate a message"
            className="absolute right-1.5 top-1/2 z-20 -translate-y-1/2"
          >
            <TooltipButton
              onClick={doGenerate}
              // Clicking while unconfigured opens AI settings, which is useful, so
              // that stays live. With nothing staged there is nothing to read, so
              // the button switches off and says so on hover.
              disabled={generating || nothingToGenerateFrom}
              aria-label={
                generating
                  ? "Generating commit message"
                  : "Generate commit message with AI"
              }
              tooltip={
                generating
                  ? "Generating commit message"
                  : !aiReady
                    ? "Set up an AI provider to generate messages"
                    : "Generate commit message with AI"
              }
              className={cn(
                "flex size-6 items-center justify-center overflow-hidden rounded-[5px] border text-sub",
                // When disabled the wrapper supplies this placement instead.
                !nothingToGenerateFrom &&
                  "absolute right-1.5 top-1/2 z-20 -translate-y-1/2",
                generating
                  ? "wyrm-ai-trigger-active cursor-wait border-transparent"
                  : justGenerated
                    ? "border-primary/25 bg-soft text-accent-text"
                    : aiReady && stagedCount > 0
                      ? "cursor-pointer border-primary/50 bg-soft text-accent-text hover:border-primary hover:bg-primary hover:text-primary-foreground"
                      : "cursor-pointer border-transparent hover:bg-panel3 hover:text-foreground",
              )}
            >
              {justGenerated && !generating ? (
                <Check size={13} strokeWidth={2.3} />
              ) : (
                <Sparkles
                  size={13}
                  className={cn(generating && "wyrm-ai-spark")}
                />
              )}
            </TooltipButton>
          </DisabledHint>
        </div>
        <Textarea
          ref={descRef}
          value={generating ? "" : desc}
          onChange={(e) => setDesc(e.target.value)}
          disabled={generating}
          placeholder={generating ? "" : "Extended description…"}
          rows={2}
          className={cn(
            "wyrm-commit-description w-full bg-background px-2.5 py-2 text-xs transition-[opacity,filter] duration-200",
            generating && "opacity-[0.18] saturate-[0.35]",
          )}
        />
        {generating && (
          <div className="wyrm-ai-stage" role="status" aria-live="polite">
            <div className="wyrm-ai-logo-wrap" aria-hidden="true">
              <div className="wyrm-ai-energy-ring" />
              <img src={logoUrl} alt="" className="wyrm-ai-logo" />
              <i className="wyrm-ai-eye wyrm-ai-eye-left" />
              <i className="wyrm-ai-eye wyrm-ai-eye-right" />
              <svg className="wyrm-ai-graph" viewBox="0 0 40 27">
                <path d="M5 5h18v17h12" />
                <circle cx="5" cy="5" r="3" />
                <circle cx="23" cy="5" r="3" />
                <circle cx="35" cy="22" r="3" />
              </svg>
            </div>
            <div className="wyrm-ai-stage-copy">
              <div className="wyrm-ai-stage-label">
                Generating commit message
              </div>
              <div className="wyrm-ai-stage-status">
                Reading {stagedCount} staged file{stagedCount === 1 ? "" : "s"}…
              </div>
              <div className="wyrm-ai-stage-detail">
                Writing a summary and description
              </div>
            </div>
          </div>
        )}
      </div>
      {(stagedCount > 0 || amend) && canAmend && (
        <label
          className={cn(
            "mb-[9px] flex cursor-pointer items-center gap-2 rounded-md border px-2.5 py-1.5 text-2xs transition-colors",
            amend
              ? "border-border bg-panel3 text-accent-text"
              : "border-border bg-panel3 text-sub hover:text-foreground",
          )}
        >
          <input
            type="checkbox"
            checked={amend}
            onChange={(e) => toggleAmend(e.target.checked)}
            className="size-3.5 accent-primary"
          />
          <Pencil size={12} strokeWidth={2} />
          <span className="font-semibold">Amend previous commit</span>
        </label>
      )}
      <ProfileSwitcher />
      <div
        className={cn(
          "flex h-[34px] w-full overflow-hidden rounded-md border transition-colors",
          canCommit
            ? "border-primary/50 bg-soft text-accent-text hover:border-primary"
            : "cursor-not-allowed border-transparent bg-panel3 text-muted-foreground",
        )}
      >
        <DisabledHint
          disabled={!canCommit}
          reason={commitBlockedReason}
          className="min-w-0 flex-1"
        >
          <button
            onClick={() => doCommit()}
            disabled={!canCommit}
            className={cn(
              "flex flex-1 items-center justify-center gap-2 text-[0.78125rem] font-semibold transition-colors",
              canCommit
                ? "cursor-pointer hover:bg-primary hover:text-primary-foreground"
                : "cursor-not-allowed",
            )}
          >
            {amend ? (
              <Pencil size={14} strokeWidth={2} />
            ) : commitButtonMode === "commit_push" ? (
              <Upload size={14} strokeWidth={2} />
            ) : (
              <GitCommitHorizontal size={15} strokeWidth={2} />
            )}
            {pushPending
              ? "Pushing…"
              : amend
                ? `Amend commit on ${currentBranch}`
                : commitButtonMode === "commit_push"
                  ? `Commit & push to ${currentBranch}`
                  : `Commit ${plural(stagedCount, "file")} to ${currentBranch}`}
          </button>
        </DisabledHint>
        <Tooltip>
          <TooltipTrigger asChild>
            <button
              role="switch"
              aria-checked={commitButtonMode === "commit_push"}
              aria-label={
                commitButtonMode === "commit_push"
                  ? "Switch to commit only"
                  : "Switch to commit and push"
              }
              onClick={() =>
                setCommitButtonMode(
                  commitButtonMode === "commit_push" ? "commit" : "commit_push",
                )
              }
              className={cn(
                "flex w-[30px] items-center justify-center border-l transition-colors",
                canCommit
                  ? "cursor-pointer border-primary/40 hover:bg-primary hover:text-primary-foreground"
                  : "cursor-pointer border-border/60 text-sub hover:bg-panel2 hover:text-foreground",
              )}
            >
              {commitButtonMode === "commit_push" ? (
                <GitCommitHorizontal size={15} strokeWidth={2} />
              ) : (
                <Upload size={14} strokeWidth={2} />
              )}
            </button>
          </TooltipTrigger>
          <TooltipContent>
            {commitButtonMode === "commit_push"
              ? "Switch to commit only (do not push)"
              : "Switch to commit and push"}
          </TooltipContent>
        </Tooltip>
      </div>
    </div>
  );
}
